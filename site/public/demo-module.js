// Version 2 exchanges bounded words. Linear memory stays private to Wasm.
const MAX_REQUEST_BYTES = 4096;
const MAX_RESPONSE_BYTES = 131072;
const EXPORTS = ["demo_abi_version", "demo_begin", "demo_write", "demo_step",
  "demo_read", "demo_state", "demo_reset"].sort();

export async function instantiate(bytes) {
  const module = await WebAssembly.compile(bytes);
  const exports = WebAssembly.Module.exports(module);
  if (WebAssembly.Module.imports(module).length !== 0
      || exports.some((entry) => entry.kind !== "function")
      || JSON.stringify(exports.map((entry) => entry.name).sort()) !== JSON.stringify(EXPORTS)) {
    throw new Error("demo module must expose only the version 2 functions, with private memory");
  }
  const instance = await WebAssembly.instantiate(module, {});
  const { demo_abi_version, demo_begin, demo_write, demo_reset,
    demo_step, demo_state, demo_read } = instance.exports;
  if (demo_abi_version() !== 2) throw new Error("unsupported demo API version");
  const encoder = new TextEncoder();
  const decoder = new TextDecoder("utf-8", { fatal: true });

  function take(length) {
    length >>>= 0;
    if (length === 0 || length > MAX_RESPONSE_BYTES) {
      throw new Error("demo boundary unavailable; discard this instance");
    }
    const bytes = new Uint8Array(length);
    for (let offset = 0; offset < length; offset += 4) {
      const word = demo_read(offset) >>> 0;
      for (let i = 0; i < 4 && offset + i < length; i++) {
        bytes[offset + i] = (word >>> (8 * i)) & 255;
      }
    }
    return JSON.parse(decoder.decode(bytes));
  }

  function tooLarge() {
    demo_begin(0); // Also discards any incomplete request.
    return { error: "request must contain 1 to 4096 UTF-8 bytes", stage: "input" };
  }

  function stepText(text) {
    if (typeof text !== "string" || text.length === 0 || text.length > MAX_REQUEST_BYTES) {
      return tooLarge();
    }
    const input = encoder.encode(text);
    if (input.length > MAX_REQUEST_BYTES) return tooLarge();
    if (demo_begin(input.length) !== 1) throw new Error("demo boundary unavailable");
    for (let offset = 0; offset < input.length; offset += 4) {
      let word = 0;
      for (let i = 0; i < 4 && offset + i < input.length; i++) {
        word |= input[offset + i] << (8 * i);
      }
      if (demo_write(word >>> 0) !== 1) throw new Error("demo boundary unavailable");
    }
    return take(demo_step());
  }

  return {
    reset: () => take(demo_reset()),
    state: () => take(demo_state()),
    step: (request) => stepText(JSON.stringify(request)),
    stepText,
  };
}
