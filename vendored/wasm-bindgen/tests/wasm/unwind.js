const wasm = new Proxy({}, { get: (_t, n) => window[n] });

// Throws an error - used to test that JS throws trigger Rust unwinding
export const js_throw_error = () => {
  throw new Error('JS throw for unwind test');
};

// Check if drop ran (reads from global set by Rust)
export const js_check_dropped = () => {
  return globalThis.unwind_drop_ran === true;
};

// Reset the drop flag
export const js_reset_dropped = () => {
  globalThis.unwind_drop_ran = false;
  globalThis.unwind_continued_after_throw = false;
};

// Trigger the unwind test by calling the Rust function
// This catches the error so we can verify it propagated
export const js_trigger_unwind_test = () => {
  wasm.rust_call_throwing_js();
};
