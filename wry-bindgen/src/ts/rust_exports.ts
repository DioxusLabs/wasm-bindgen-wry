import { DataEncoder } from "./encoding";
import { handleBinaryResponse, MessageType, sync_request_binary, CALL_EXPORT_FN_ID } from "./ipc";

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

  const response = sync_request_binary("/handler", encoder.finalize());
  handleBinaryResponse(response);
});

/**
 * Call an exported Rust method by name.
 * This is exposed as window.__wryCallExport for generated class methods to use.
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

  const response = sync_request_binary("/handler", encoder.finalize());
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
 * Uses the generated class from JsClassSpec if available, otherwise falls back to Proxy.
 */
function createWrapper(handle: number, className: string): object {
  // Try to use the generated class if available
  const ClassConstructor = (window as any)[className];
  if (ClassConstructor && typeof ClassConstructor.__wrap === 'function') {
    return ClassConstructor.__wrap(handle);
  }

  // Fallback: Create wrapper object with the handle stored (legacy Proxy approach)
  // This will be removed once all classes are migrated to use JsClassSpec
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

// Expose callExport and exportRegistry as window globals for generated classes to use
(window as any).__wryCallExport = callExport;
(window as any).__wryExportRegistry = exportRegistry;

/**
 * RustExports manager - provides wrapper creation for exported structs.
 */
const rustExports = {
  createWrapper,
  callExport,
};

export { rustExports, createWrapper, callExport };
