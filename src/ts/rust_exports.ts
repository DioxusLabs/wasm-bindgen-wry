import { DataEncoder, DataDecoder } from "./encoding";
import { handleBinaryResponse, MessageType, sync_request_binary, CALL_EXPORT_FN_ID, DROP_NATIVE_REF_FN_ID } from "./ipc";

/**
 * FinalizationRegistry to notify Rust when exported object wrappers are GC'd.
 * The callback sends a drop message to Rust with the object handle.
 */
const exportRegistry = new FinalizationRegistry<{ handle: number; className: string }>((info) => {
  // Build Evaluate message to drop the object: call ClassName::__drop with handle
  const encoder = new DataEncoder();
  encoder.pushU8(MessageType.Evaluate);
  encoder.pushU32(CALL_EXPORT_FN_ID);
  // Encode the export name as a string
  const dropName = `${info.className}::__drop`;
  encoder.pushStr(dropName);
  // Encode the handle as u32
  encoder.pushU32(info.handle);

  const response = sync_request_binary("wry://handler", encoder.finalize());
  handleBinaryResponse(response);
});

/**
 * Call an exported Rust method by name.
 */
function callExport(exportName: string, ...args: any[]): any {
  window.jsHeap.pushBorrowFrame();

  const encoder = new DataEncoder();
  encoder.pushU8(MessageType.Evaluate);
  encoder.pushU32(CALL_EXPORT_FN_ID);
  // Encode the export name as a string
  encoder.pushStr(exportName);
  // Encode arguments - for now, we assume they're already u32 handles or primitives
  for (const arg of args) {
    if (typeof arg === "number") {
      encoder.pushU32(arg);
    } else {
      throw new Error(`Unsupported argument type: ${typeof arg}`);
    }
  }

  const response = sync_request_binary("wry://handler", encoder.finalize());
  const decoder = handleBinaryResponse(response);

  window.jsHeap.popBorrowFrame();

  // If we have response data, try to decode it
  // For now, try to decode as i32 if there's u32 data available
  if (decoder && decoder.hasMoreU32()) {
    return decoder.takeI32();
  }

  return undefined;
}

/**
 * Create a JavaScript wrapper object for a Rust exported struct.
 * The wrapper has methods that call back into Rust.
 */
function createWrapper(handle: number, className: string): object {
  // Create wrapper object with the handle stored
  const wrapper: any = {
    __handle: handle,
    __className: className,
  };

  // Create a Proxy to intercept method calls and property access
  const proxy = new Proxy(wrapper, {
    get(target, prop) {
      if (prop === "__handle" || prop === "__className") {
        return target[prop];
      }
      // Skip Symbol properties and common JS properties
      if (typeof prop === "symbol" || prop === "then" || prop === "toJSON") {
        return undefined;
      }
      // Return a function that calls the Rust export when invoked
      // For s.method(), this returns a function that is then called
      // For s.property (getter), the caller should use s.property() or we define real getters
      return (...args: any[]) => {
        const exportName = `${className}::${String(prop)}`;
        // Pass the handle as the first argument (for self methods)
        return callExport(exportName, handle, ...args);
      };
    },
  });

  // Register for GC notification
  exportRegistry.register(proxy, { handle, className });

  return proxy;
}

/**
 * RustExports manager - provides wrapper creation for exported structs.
 */
const rustExports = {
  createWrapper,
  callExport,
};

export { rustExports, createWrapper, callExport };
