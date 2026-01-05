import { DataEncoder } from "./encoding";
import { handleBinaryResponse, MessageType, sync_request_binary, DROP_NATIVE_REF_FN_ID } from "./ipc";
import { TypeClass } from "./types";

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
  private paramTypes: TypeClass[];
  private returnType: TypeClass;

  constructor(fnId: number, paramTypes: TypeClass[], returnType: TypeClass) {
    console.log(`Creating RustFunction wrapper for fnId ${fnId}`);
    this.fnId = fnId;
    this.paramTypes = paramTypes;
    this.returnType = returnType;
    // Register this instance so Rust is notified when we're GC'd
    nativeRefRegistry.register(this, fnId);
  }

  call(...args: any[]): any {
    // Push a borrow frame before encoding args - nested calls won't clear our borrowed refs
    window.jsHeap.pushBorrowFrame();
    console.log("Pushed borrow frame for RustFunction call");

    console.log(`Calling Rust function ID ${this.fnId} with args:`, args);
    // Build Evaluate message: [0, fn_id]
    const encoder = new DataEncoder();
    encoder.pushU8(MessageType.Evaluate);
    encoder.pushU32(0); // Call argument function
    encoder.pushU64(this.fnId);
    // Encode arguments (may put borrowed refs on the borrow stack)
    for (let i = 0; i < this.paramTypes.length; i++) {
      this.paramTypes[i].encode(encoder, args[i]);
    }

    // Send to Rust and get response (Rust may call back to JS during this)
    const response = sync_request_binary("wry://handler", encoder.finalize());
    const result = handleBinaryResponse(response)!;
    console.log("Received response from RustFunction call", result);

    // Pop the borrow frame - clears borrowed refs from this call
    console.log("Finished RustFunction call, popping borrow frame");
    window.jsHeap.popBorrowFrame();

    // Decode return value
    return this.returnType.decode(result);
  }
}

export { RustFunction };