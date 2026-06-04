import {
  DROP_NATIVE_REF_FN_ID,
  sendEvaluateToRust,
} from "./ipc";
import { TypeClass } from "./types";

enum RustFunctionPolicy {
  RustOwned = 0,
  JsOwned = 1,
  JsOwnedOnce = 2,
}

/**
 * FinalizationRegistry to notify Rust when RustFunction wrappers are GC'd.
 * The callback sends a drop message to Rust with the fnId.
 */
function dropNativeRef(fnId: number): void {
  // Build Evaluate message to drop native ref: [DROP_NATIVE_REF_FN_ID, fn_id]
  sendEvaluateToRust((encoder) => {
    encoder.pushU32(DROP_NATIVE_REF_FN_ID);
    encoder.pushU32(fnId);
  });
}

const nativeRefRegistry = new FinalizationRegistry<number>((fnId: number) => {
  dropNativeRef(fnId);
});

/**
 * Rust function wrapper that can call back into Rust.
 * Registered with FinalizationRegistry so Rust is notified when this is GC'd.
 */
class RustFunction {
  declare private fnId: number;
  declare private paramTypes: TypeClass[];
  declare private returnType: TypeClass;
  declare private dropAfterCall: boolean;
  declare private disposed: boolean;
  declare private activeCalls: number;
  declare private dropNativeWhenIdle: boolean;
  declare private finalizerToken: object | null;

  constructor(
    fnId: number,
    paramTypes: TypeClass[],
    returnType: TypeClass,
    policy: RustFunctionPolicy
  ) {
    this.fnId = fnId;
    this.paramTypes = paramTypes;
    this.returnType = returnType;
    this.dropAfterCall = policy === RustFunctionPolicy.JsOwnedOnce;
    this.disposed = false;
    this.activeCalls = 0;
    this.dropNativeWhenIdle = false;
    this.finalizerToken = null;
    if (policy !== RustFunctionPolicy.RustOwned) {
      this.finalizerToken = {};
      // Register this instance so Rust is notified when we're GC'd
      nativeRefRegistry.register(this, fnId, this.finalizerToken);
    }
  }

  call(...args: any[]): any {
    if (this.disposed) {
      throw new Error("Rust function has already been dropped");
    }

    this.activeCalls++;
    // Push a borrow frame before encoding args - nested calls won't clear our borrowed refs
    window.jsHeap.pushBorrowFrame();
    let result: ReturnType<typeof sendEvaluateToRust> = null;
    try {
      // Send to Rust and get response (Rust may call back to JS during this)
      result = sendEvaluateToRust((encoder) => {
        encoder.pushU32(0); // Call argument function
        encoder.pushU32(this.fnId);
        // Encode arguments (may put borrowed refs on the borrow stack)
        for (let i = 0; i < this.paramTypes.length; i++) {
          this.paramTypes[i].encode(encoder, args[i]);
        }
      });
    } finally {
      // Pop the borrow frame - clears borrowed refs from this call
      window.jsHeap.popBorrowFrame();
      this.activeCalls--;
      this.dropNativeRefIfIdle();
    }

    // Decode return value
    const decoded = this.returnType.decode(result!);
    if (result && !result.isEmpty()) {
      throw new Error("Unprocessed data remaining after RustFunction call");
    }
    if (this.dropAfterCall && this.finalizerToken) {
      nativeRefRegistry.unregister(this.finalizerToken);
      this.finalizerToken = null;
      this.disposed = true;
    }
    return decoded;
  }

  disposeFromRust(): void {
    if (this.disposed) {
      return;
    }

    this.disposed = true;

    if (this.dropAfterCall && this.activeCalls !== 0) {
      if (this.finalizerToken) {
        nativeRefRegistry.unregister(this.finalizerToken);
        this.finalizerToken = null;
      }
      return;
    }

    this.dropNativeWhenIdle = true;
    this.dropNativeRefIfIdle();
  }

  private dropNativeRefIfIdle(): void {
    if (!this.dropNativeWhenIdle || this.activeCalls !== 0) {
      return;
    }

    this.dropNativeWhenIdle = false;
    if (this.finalizerToken) {
      nativeRefRegistry.unregister(this.finalizerToken);
      this.finalizerToken = null;
    }
    dropNativeRef(this.fnId);
  }
}

export { RustFunction, RustFunctionPolicy };
