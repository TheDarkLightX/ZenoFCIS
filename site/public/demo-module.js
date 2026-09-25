// Loads a demo module and wraps its C ABI.
//
// Input crosses as a buffer from `demo_alloc`; a result is a buffer whose
// first four bytes hold the length of the JSON that follows, freed with
// `demo_free` once read. The same code serves the page and the headless test.

export async function instantiate(bytes) {
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const { memory, demo_alloc, demo_free, demo_reset, demo_step, demo_state } = instance.exports;
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();

  // The memory's buffer is re-read on every access: a call may grow the
  // memory, which detaches earlier views.
  function take(pointer) {
    const length = new DataView(memory.buffer).getUint32(pointer, true);
    const text = decoder.decode(new Uint8Array(memory.buffer, pointer + 4, length));
    demo_free(pointer, length + 4);
    return JSON.parse(text);
  }

  function stepText(text) {
    const input = encoder.encode(text);
    const pointer = demo_alloc(input.length);
    new Uint8Array(memory.buffer, pointer, input.length).set(input);
    const result = demo_step(pointer, input.length);
    demo_free(pointer, input.length);
    return take(result);
  }

  return {
    reset: () => take(demo_reset()),
    state: () => take(demo_state()),
    step: (request) => stepText(JSON.stringify(request)),
    stepText,
  };
}
