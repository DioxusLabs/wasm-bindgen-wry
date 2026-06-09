const wasm = new Proxy({}, { get: (_t, n) => window[n] });

// Get the current WebAssembly memory buffer size in bytes
export const get_memory_byte_length = function () {
  return wasm.__wasm.memory.buffer.byteLength;
};
