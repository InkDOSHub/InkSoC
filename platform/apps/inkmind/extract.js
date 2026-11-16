// AI Chat attachments: turns a file into text for the model, on the device.
// PDF with pdf.js, Word/PowerPoint/Excel/OpenDocument with JSZip (both already in the Hub, InkDOS apps), the rest as text.
const PDFJS = new URL("/inkdos/apps/pdf/vendor/pdfjs/pdf.min.mjs", import.meta.url).href;
const PDFJS_WORKER = new URL("/inkdos/apps/pdf/vendor/pdfjs/pdf.worker.min.mjs", import.meta.url).href;
const JSZIP = new URL("/inkdos/apps/documents/vendor/jszip.min.js", import.meta.url).href;

let zipLoading = null;
function jszip() {
  if (window.JSZip) return Promise.resolve(window.JSZip);
  if (!zipLoading) zipLoading = new Promise((resolve, reject) => {
    const s = document.createElement("script");
    s.src = JSZIP; s.onload = () => resolve(window.JSZip); s.onerror = () => reject(new Error("JSZip did not load"));
    document.head.appendChild(s);
  });
  return zipLoading;
}
const xml = (t) => new DOMParser().parseFromString(t, "application/xml");
const textsOf = (doc, tag) => Array.from(doc.getElementsByTagName(tag)).map((n) => n.textContent);

async function pdf(file) {
  const lib = await import(PDFJS);
  lib.GlobalWorkerOptions.workerSrc = PDFJS_WORKER;
  const d = await lib.getDocument({ data: new Uint8Array(await file.arrayBuffer()), isEvalSupported: false }).promise;
  const partes = [];
  for (let i = 1; i <= d.numPages; i++) {
    const c = await (await d.getPage(i)).getTextContent();
    partes.push("[page " + i + "]\n" + c.items.map((x) => x.str + (x.hasEOL ? "\n" : " ")).join(""));
  }
  return partes.join("\n\n");
}
async function office(file, ext) {
  const z = await (await jszip()).loadAsync(await file.arrayBuffer());
  const readEntry = (n) => (z.file(n) ? z.file(n).async("string") : Promise.resolve(""));
  const sorted = (re) => Object.keys(z.files).filter((n) => re.test(n)).sort((a, b) => +a.match(/(\d+)\.xml$/)[1] - +b.match(/(\d+)\.xml$/)[1]);
  if (ext === "docx") return Array.from(xml(await readEntry("word/document.xml")).getElementsByTagName("w:p")).map((p) => textsOf(p, "w:t").join("")).join("\n");
  if (ext === "pptx") {
    const out = [];
    for (const n of sorted(/^ppt\/slides\/slide\d+\.xml$/)) out.push("[slide " + n.match(/(\d+)\.xml$/)[1] + "]\n" + textsOf(xml(await readEntry(n)), "a:t").join(" "));
    return out.join("\n\n");
  }
  if (ext === "xlsx") {
    const comp = textsOf(xml(await readEntry("xl/sharedStrings.xml")), "si");
    const sheets = Array.from(xml(await readEntry("xl/workbook.xml")).getElementsByTagName("sheet")).map((s) => s.getAttribute("name"));
    const out = [];
    for (const [k, n] of sorted(/^xl\/worksheets\/sheet\d+\.xml$/).entries()) {
      const rows = Array.from(xml(await readEntry(n)).getElementsByTagName("row")).map((r) => Array.from(r.getElementsByTagName("c")).map((c) => {
        const v = c.getElementsByTagName("v")[0], t = c.getAttribute("t");
        if (t === "s" && v) return comp[+v.textContent] || "";
        if (t === "inlineStr") return textsOf(c, "t").join("");
        return v ? v.textContent : "";
      }).join("\t"));
      out.push("[sheet " + (sheets[k] || k + 1) + "]\n" + rows.join("\n"));
    }
    return out.join("\n\n");
  }
  // OpenDocument (odt, ods, odp): paragraphs and cells of content.xml
  const d = xml(await readEntry("content.xml"));
  return Array.from(d.getElementsByTagName("*")).filter((n) => /^text:(p|h)$/.test(n.tagName)).map((n) => n.textContent).join("\n");
}

export const isImage = (file) => /^image\//.test(file.type) || /\.(png|jpe?g|webp|gif|bmp)$/i.test(file.name);

export async function extractText(file) {
  const ext = (file.name.split(".").pop() || "").toLowerCase();
  if (ext === "pdf") return pdf(file);
  if (/^(docx|pptx|xlsx|odt|ods|odp)$/.test(ext)) return office(file, ext);
  const t = await file.text();
  if (/^html?$/.test(ext)) return new DOMParser().parseFromString(t, "text/html").body.innerText;
  if (/\u0000/.test(t.slice(0, 4000))) throw new Error(file.name + ": this kind of file cannot be read as text");
  return t;
}
