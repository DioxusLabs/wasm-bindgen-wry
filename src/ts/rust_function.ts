import { DataEncoder } from "./encoding";
import { handleBinaryResponse, MessageType, sync_request_binary, DROP_NATIVE_REF_FN_ID } from "./ipc";

/**
 * FinalizationRegistry to notify Rust when RustFunction wrappers are GC'd.
 * The callback sends a drop message to Rust with the fnId.
 */
const nativeRefRegistry = new FinalizationRegistry<number>((fnId: number) => {
  // Build Evaluate message to drop native ref: [DROP_NATIVE_REF_FN_ID, fn_id]
  const encoder = new DataEncoder();
  encoder.pushU8(MessageType.Evaluate);
  encoder.pushU32(DROP_NATIVE_REF_FN_ID);
  encoder.pushU64(fnId);

  const response = sync_request_binary("wry://handler", encoder.finalize());
  handleBinaryResponse(response);
});

/**
 * Rust function wrapper that can call back into Rust.
 * Registered with FinalizationRegistry so Rust is notified when this is GC'd.
 */
class RustFunction {
  private fnId: number;

  constructor(fnId: number) {
    this.fnId = fnId;
    // Register this instance so Rust is notified when we're GC'd
    nativeRefRegistry.register(this, fnId);
  }

  call(): unknown {
    // Build Evaluate message: [0, fn_id]
    const encoder = new DataEncoder();
    encoder.pushU8(MessageType.Evaluate);
    encoder.pushU32(0); // Call argument function
    encoder.pushU64(this.fnId);

    const response = sync_request_binary("wry://handler", encoder.finalize());
    return handleBinaryResponse(response);
  }
}

export { RustFunction };