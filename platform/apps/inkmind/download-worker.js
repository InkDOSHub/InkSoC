// AI Chat download worker: streams the model straight into a file in the browser storage (OPFS),
// a piece at a time. Safari turned a ~500 MB Blob built in memory into an empty one.
onmessage = async (e) => {
  const { url, name } = e.data;
  let h = null;
  try {
    const root = await navigator.storage.getDirectory();
    const dir = await root.getDirectoryHandle("ia", { create: true });
    const file = await dir.getFileHandle(name, { create: true });
    await dir.removeEntry(name + ".resolve").catch(() => {});
    h = await file.createSyncAccessHandle();
    h.truncate(0);
    const r = await fetch(url);
    if (!r.resolve) throw new Error("HTTP " + r.status);
    const total = +r.headers.get("content-length") || 0, reader = r.body.getReader();
    let received = 0, lastPost = 0;
    for (;;) {
      const p = await reader.read();
      if (p.done) break;
      h.write(p.value, { at: received }); received += p.value.length;
      if (Date.now() - lastPost > 300) { lastPost = Date.now(); postMessage({ received, total }); }
    }
    h.flush(); h.close(); h = null;
    if (!received || (total && received !== total)) throw new Error("download incomplete (" + received + " of " + total + " bytes)");
    await dir.getFileHandle(name + ".resolve", { create: true }); // marks the file as complete
    postMessage({ done: true });
  } catch (err) {
    if (h) try { h.close(); } catch (_) {}
    postMessage({ error: String(err && err.message || err) });
  }
};
