import {
  CALL_EXPORT_FN_ID,
  sendEvaluateToRust,
} from "./ipc";
import { parseTypeDef, TypeClass } from "./types";

function typeFromBytes(bytes: number[]): TypeClass {
  const offset = { value: 0 };
  const ty = parseTypeDef(new Uint8Array(bytes), offset);
  if (offset.value !== bytes.length) {
    throw new Error(`Unprocessed export type data: ${bytes.length - offset.value} bytes`);
  }
  return ty;
}

const U32_TYPE_DEF = [4];

/**
 * FinalizationRegistry to notify Rust when exported object wrappers are GC'd.
 * The callback sends a drop message to Rust with the object handle.
 */
const exportRegistry = new FinalizationRegistry<{ handle: number; className: string }>((info) => {
  callExport(`${info.className}::__drop`, [U32_TYPE_DEF], null, [info.handle]);
});

/**
 * Call an exported Rust method by name.
 * This is exposed as window.__wryCallExport for generated class methods.
 */
function callExport(
  exportName: string,
  argTypeDefs: number[][],
  returnTypeDef: number[] | null,
  args: any[],
): any {
  if (argTypeDefs.length !== args.length) {
    throw new Error(
      `Export ${exportName} expected ${argTypeDefs.length} arguments but got ${args.length}`,
    );
  }

  window.jsHeap.pushBorrowFrame();

  try {
    const decoder = sendEvaluateToRust((encoder) => {
      encoder.pushU32(CALL_EXPORT_FN_ID);
      encoder.pushStr(exportName);

      for (let i = 0; i < args.length; i++) {
        typeFromBytes(argTypeDefs[i]).encode(encoder, args[i]);
      }
    });

    if (returnTypeDef === null) {
      if (decoder && !decoder.isEmpty()) {
        throw new Error(`Unprocessed data remaining after export ${exportName}`);
      }
      return undefined;
    }

    if (!decoder) {
      throw new Error(`Missing response data for export ${exportName}`);
    }

    const result = typeFromBytes(returnTypeDef).decode(decoder);
    if (!decoder.isEmpty()) {
      throw new Error(`Unprocessed data remaining after export ${exportName}`);
    }
    return result;
  } finally {
    window.jsHeap.popBorrowFrame();
  }
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
      if (prop === "free") {
        return () => {
          const handle = target.__handle;
          target.__handle = 0;
          if (handle !== 0) {
            callExport(`${className}::__drop`, [U32_TYPE_DEF], null, [handle]);
          }
        };
      }
      // Skip Symbol properties and common JS properties
      if (typeof prop === "symbol" || prop === "then" || prop === "toJSON") {
        return undefined;
      }
      return () => {
        const exportName = `${className}::${String(prop)}`;
        throw new Error(
          `Cannot call ${exportName} through a fallback wrapper without generated type metadata`,
        );
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
