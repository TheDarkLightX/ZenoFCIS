/* The engine of the portable desk. It runs in a worker of the page: Python itself as WebAssembly, with ZAL's
   unmodified sources unpacked beside portable_desk.py. It passes text both ways and reads none of it.
   Every address it requests is one of the page's own files, beside this one. */
import { loadPyodide } from "./pyodide/pyodide.mjs";

const beside = (name) => new URL(name, import.meta.url).href;
let exchange = null;

const started = (async () => {
  // Asked first, because a runtime that is refused its WebAssembly can leave its own start waiting for ever.
  try { new WebAssembly.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0])); }
  catch (error) { throw new Error("This page is not allowed to run WebAssembly here, and ZAL's checker runs as WebAssembly."); }
  const python = await loadPyodide({ indexURL: beside("./pyodide/") });
  const sources = await fetch(beside("./zal_sources.zip"));
  if (!sources.ok) throw new Error("ZAL's sources are missing from this copy of the desk");
  python.unpackArchive(await sources.arrayBuffer(), "zip", { extractDir: "/zal" });
  python.runPython("import sys\nsys.dont_write_bytecode = True\nsys.path.insert(0, '/zal')");
  exchange = python.pyimport("portable_desk").exchange;
  const told = JSON.parse(exchange(JSON.stringify({ route: "about" })));
  if (!told.ok) throw new Error(told.error);
  return { ...told.reply, runtime: python.version };
})();

started.then((about) => postMessage({ type: "ready", about }),
  (error) => postMessage({ type: "failed", reason: String((error && error.message) || error).slice(0, 2000) }));

self.onmessage = async (event) => {
  const { id, request } = event.data;
  let text;
  try { await started; text = exchange(request); }
  catch (error) { text = JSON.stringify({ ok: false, error: String((error && error.message) || error).slice(0, 2000), unexpected: true }); }
  postMessage({ type: "reply", id, text });
};
