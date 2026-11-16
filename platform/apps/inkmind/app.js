// AI Chat (experimental): small GGUF models running on the device with wllama (llama.cpp in wasm).
// The model comes from Hugging Face once, through the site's Worker (worker.js; Safari fails the direct download), and is kept as a file in the browser storage (OPFS).
import { Wllama } from "./wllama/index.js";
import { extractText, isImage } from "./extract.js";

const MODELS = [
  { repo: "Qwen/Qwen2.5-0.5B-Instruct-GGUF", file: "qwen2.5-0.5b-instruct-q4_k_m.gguf" },
  { repo: "bartowski/Llama-3.2-1B-Instruct-GGUF", file: "Llama-3.2-1B-Instruct-Q4_K_M.gguf" },
  { repo: "Qwen/Qwen2.5-1.5B-Instruct-GGUF", file: "qwen2.5-1.5b-instruct-q4_k_m.gguf" }
];
const $ = (id) => document.getElementById(id);
const chat = $("chat"), statusLine = $("status"), text = $("text"), sendButton = $("send"), modelSelect = $("model");
// while loading, the engine's last log line is shown, so a stall in XeOS shows where it stopped
let showLog = false;
const logLine = (...a) => { console.log(...a); if (showLog) notice("Loading the model… " + a.map(String).join(" ").slice(0, 160)); };
// wllama 2.3.7 on purpose: 2.4.0 and later need Memory64, which Safari does not have
const wllama = new Wllama({
  "single-thread/wllama.wasm": new URL("./wllama/single-thread/wllama.wasm", import.meta.url).href,
  "multi-thread/wllama.wasm": new URL("./wllama/multi-thread/wllama.wasm", import.meta.url).href
},
  { logger: { debug: logLine, log: logLine, warn: logLine, error: logLine } });
let loadedIndex = null, busy = false, stopper = null;
// Windows app (window.inkHubNativo.ia, method names set by the Hub: modelos, iniciar, conversar, parar, site)
// Windows app: every model, from the lightest to the heaviest, runs on native llama.cpp (windows/hub/ia.js)
const native = window.inkHubNativo && window.inkHubNativo.ia;
if (native) native.modelos().then((list) => {
  modelSelect.textContent = "";
  for (const m of list) modelSelect.add(new Option(m.nome + " (" + m.gb + " GB)", m.id));
  modelSelect.value = "qwen3b"; // suggested start (1.9 GB); bigger ones for who wants more
}).catch(() => {});
const history = [{ role: "system", content: "You are a helpful assistant. Answer briefly, in the user's language." }];

function bubble(kind, t) {
  const p = document.createElement("div");
  p.className = "msg " + kind; p.textContent = t;
  chat.appendChild(p); chat.scrollTop = chat.scrollHeight;
  return p;
}
function notice(t) { statusLine.textContent = t; chat.appendChild(statusLine); chat.scrollTop = chat.scrollHeight; }

async function loadSelectedModel() {
  const i = +modelSelect.value;
  if (loadedIndex === i) return;
  if (loadedIndex !== null) await wllama.exit();
  loadedIndex = null;
  notice("Loading the model…");
  const m = MODELS[i], url = new URL("./modelos/" + m.file, import.meta.url).href;
  // kept as a file in the browser storage (OPFS), written by download-worker.js a piece at a time
  const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle("ia", { create: true });
  const complete = await dir.getFileHandle(m.file + ".resolve").then(() => true, () => false);
  let file = complete ? await dir.getFileHandle(m.file).then((h) => h.getFile()).catch(() => null) : null;
  if (!file || file.size < 1e7) {
    await new Promise((resolve, reject) => {
      const w = new Worker("download-worker.js");
      w.onmessage = (e) => {
        const d = e.data;
        if (d.received) notice("Downloading the model (only the first time)… " + (d.total ? Math.round(d.received / d.total * 100) + "%" : Math.round(d.received / 1048576) + " MB"));
        if (d.done) { w.terminate(); resolve(); }
        if (d.error) { w.terminate(); reject(new Error(d.error)); }
      };
      w.onerror = (e) => { w.terminate(); reject(new Error(e.message || "download worker failed")); };
      w.postMessage({ url: url + "?inkoff=1", name: m.file });
    });
    file = await (await dir.getFileHandle(m.file)).getFile();
  }
  if (window.caches) caches.delete("inkdoshub-ia").catch(() => {}); // old copies from the first version
  const blob = file;
  notice("Loading the model… (" + Math.round(blob.size / 1048576) + " MB, " + (navigator.hardwareConcurrency || "?") + " cores)");
  showLog = true;
  try { await wllama.loadModel([blob], { n_ctx: 4096 }); } finally { showLog = false; }
  loadedIndex = i;
  notice(wllama.isMultithread() ? "Model ready." : "Model ready (single thread, slower).");
}

// attachments (+ button): text of files on every device; images and web pages only in the Windows app
let attachments = [];
const ATTACH_LIMIT = native ? 18000 : 8000;      // characters of attachments per message
const CONTEXT_BUDGET = native ? 22000 : 11000;  // characters of conversation sent to the model (context 8192 / 4096 tokens)
if (native) $("file-input").accept += ",image/*";
$("add-files").addEventListener("click", () => $("file-input").click());
const dataUrl = (f) => new Promise((resolve, reject) => { const r = new FileReader(); r.onload = () => resolve(r.result); r.onerror = () => reject(r.error); r.readAsDataURL(f); });
function showAttachments() {
  const box = $("attachments"); box.textContent = "";
  attachments.forEach((x, i) => {
    const c = document.createElement("div"); c.className = "attachment";
    const n = document.createElement("span"); n.textContent = (x.image ? "🖼 " : "📄 ") + x.name;
    const b = document.createElement("button"); b.type = "button"; b.textContent = "✕"; b.setAttribute("aria-label", "Remove");
    b.onclick = () => { attachments.splice(i, 1); showAttachments(); };
    c.append(n, b); box.appendChild(c);
  });
}
$("file-input").addEventListener("change", async () => {
  for (const f of Array.from($("file-input").files)) {
    try {
      if (isImage(f)) {
        if (!native) throw new Error("images only in the Windows app");
        attachments.push({ name: f.name, image: await dataUrl(f) });
      } else {
        notice("Reading " + f.name + "…");
        attachments.push({ name: f.name, text: (await extractText(f)).trim() });
        statusLine.remove();
      }
    } catch (err) { notice("Could not read " + f.name + ": " + (err && err.message || err)); }
  }
  $("file-input").value = ""; showAttachments();
});
// what is sent: the newest messages that fit in the budget; images only in the last message
const lengthOf = (m) => (typeof m.content === "string" ? m.content : m.content[0].text).length;
function buildMessages() {
  const out = []; let used = 0;
  for (let i = history.length - 1; i > 0; i--) {
    let m = history[i];
    if (out.length && typeof m.content !== "string") m = { role: m.role, content: m.content[0].text };
    used += lengthOf(m);
    if (out.length && used > CONTEXT_BUDGET) break;
    out.unshift(m);
  }
  return [history[0]].concat(out);
}

$("form").addEventListener("submit", async (e) => {
  e.preventDefault();
  if (busy) { if (stopper) stopper.abort(); return; }
  const t = text.value.trim();
  if (!t && !attachments.length) return;
  busy = true; sendButton.textContent = "Stop"; text.value = "";
  const sent = attachments; attachments = []; showAttachments();
  bubble("user", (t || "") + (sent.length ? (t ? "\n" : "") + sent.map((x) => (x.image ? "🖼 " : "📄 ") + x.name).join("\n") : ""));
  let user = null;
  try {
    // Windows app: web pages whose address is in the message are read by the Hub
    if (native && native.site) for (const url of (t.match(/https?:\/\/[^\s<>"']+/g) || []).slice(0, 3)) {
      notice("Reading " + url + "…");
      try { const p = await native.site(url); sent.push({ name: url, text: (p.titulo ? p.titulo + "\n\n" : "") + p.texto }); }
      catch (err) { sent.push({ name: url, text: "(could not open this page: " + (err && err.message || err) + ")" }); }
    }
    let body = t || "Analyze the attached content.", room = ATTACH_LIMIT;
    for (const x of sent.filter((y) => y.text != null)) {
      let tx = x.text;
      if (tx.length > room) tx = tx.slice(0, Math.max(0, room)) + "\n[… cut: only the first " + Math.max(0, room) + " of " + x.text.length + " characters fit]";
      room -= tx.length;
      body += "\n\n[Attached: " + x.name + "]\n" + tx;
    }
    const images = sent.filter((y) => y.image);
    if (images.length && native && !/vl/.test(modelSelect.value)) { modelSelect.value = "qwenvl3b"; notice("Images: using Qwen 2.5 VL 3B, which can see images."); }
    user = { role: "user", content: images.length ? [{ type: "text", text: body }].concat(images.map((y) => ({ type: "image_url", image_url: { url: y.image } }))) : body };
    history.push(user);
    if (!native) await loadSelectedModel();
    if (native) await native.iniciar(modelSelect.value, notice);
    statusLine.remove();
    const reply = bubble("ai", "…");
    let r = "";
    if (native) {
      stopper = { abort: () => native.parar(), signal: {} };
      r = await native.conversar(buildMessages(), (x) => { reply.textContent = x; chat.scrollTop = chat.scrollHeight; });
      reply.textContent = r || "…";
    } else {
      stopper = new AbortController();
      await wllama.createChatCompletion(buildMessages(), {
        nPredict: 512, sampling: { temp: 0.7, top_p: 0.9 }, abortSignal: stopper.signal,
        onNewToken: (_t, _p, soFar) => { r = soFar; reply.textContent = r; chat.scrollTop = chat.scrollHeight; }
      }).catch((err) => { if (!stopper.signal.aborted) throw err; });
    }
    history.push({ role: "assistant", content: r || "…" });
  } catch (err) {
    if (user && history[history.length - 1] === user) history.pop();
    notice("Error: " + (err && err.message || err) + (native ? "" : ". Try Qwen 0.5B."));
  }
  busy = false; stopper = null; sendButton.textContent = "Send";
});
text.addEventListener("keydown", (e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); $("form").requestSubmit(); } });
modelSelect.addEventListener("change", () => { if (!busy) notice("The model changes on the next message."); });
