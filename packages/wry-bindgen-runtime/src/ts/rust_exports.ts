import {
  CALL_EXPORT_FN_ID,
  sendEvaluateToRust,
} from "./ipc";
import { parseTypeDef, RustValueType, MutArrayType, TypeClass } from "./types";
import { DataDecoder } from "./encoding";

function typeFromBytes(bytes: number[]): TypeClass {
  const offset = { value: 0 };
  const ty = parseTypeDef(new Uint8Array(bytes), offset);
  if (offset.value !== bytes.length) {
    throw new Error(`Unprocessed export type data: ${bytes.length - offset.value} bytes`);
  }
  return ty;
}

const U32_TYPE_DEF = [4];
const UNDEFINED_TYPE_DEF = [0];

function isUndefinedTypeDef(typeDef: number[]): boolean {
  return typeDef.length === 1 && typeDef[0] === 0;
}

/**
 * FinalizationRegistry to notify Rust when exported object wrappers are GC'd.
 * The callback sends a drop message to Rust with the object handle.
 */
const exportRegistry = new FinalizationRegistry<{ drops: [string, number][] }>((info) => {
  // An inheritance descendant carries the own object plus one ancestor view per
  // ancestor (each a clone of the shared parent cell), so drop every backing
  // object when the wrapper is collected.
  for (const [className, handle] of info.drops) {
    if (handle !== 0) {
      callExport(`${className}::__drop`, [U32_TYPE_DEF], UNDEFINED_TYPE_DEF, [handle]);
    }
  }
});

/**
 * Copy each `&mut [T]` argument's write-back payload (appended to the response
 * after the return value, in argument order) into the caller's original array.
 */
function copyBackMutArrays(
  decoder: DataDecoder,
  argTypes: TypeClass[],
  args: any[],
): void {
  for (let i = 0; i < argTypes.length; i++) {
    const argType = argTypes[i];
    if (argType instanceof MutArrayType) {
      argType.copyBack(decoder, args[i]);
    }
  }
}

/**
 * Call an exported Rust method by name.
 * This is exposed as window.__wryCallExport for generated class methods.
 */
function callExport(
  exportName: string,
  argTypeDefs: number[][],
  returnTypeDef: number[],
  args: any[],
): any {
  if (argTypeDefs.length !== args.length) {
    throw new Error(
      `Export ${exportName} expected ${argTypeDefs.length} arguments but got ${args.length}`,
    );
  }

  window.jsHeap.pushBorrowFrame();

  // Parse the argument types once so by-value Rust struct arguments
  // (`RustValueType`) can be zeroed out after the call: passing such a wrapper
  // transfers ownership to Rust, mirroring wasm-bindgen's `__destroy_into_raw`.
  const argTypes = argTypeDefs.map(typeFromBytes);

  try {
    const decoder = sendEvaluateToRust((encoder) => {
      encoder.pushU32(CALL_EXPORT_FN_ID);
      encoder.pushStr(exportName);

      for (let i = 0; i < args.length; i++) {
        argTypes[i].encode(encoder, args[i]);
      }
    });

    // Rust has now taken ownership of every by-value struct argument, so zero
    // the wrapper's handle. A later method/getter/setter call on it sees the
    // zeroed handle and throws "Attempt to use a moved value".
    for (let i = 0; i < args.length; i++) {
      if (argTypes[i] instanceof RustValueType && args[i] != null) {
        args[i].__handle = 0;
      }
    }

    if (!decoder && isUndefinedTypeDef(returnTypeDef)) {
      // A `&mut [T]` argument still needs its write-back payload, so a missing
      // response is only valid when there are no mutable-array arguments.
      if (argTypes.some((t) => t instanceof MutArrayType)) {
        throw new Error(`Missing response data for export ${exportName}`);
      }
      return undefined;
    }

    if (!decoder) {
      throw new Error(`Missing response data for export ${exportName}`);
    }

    // A return value transfers ownership to JS, so decode it in take mode: owned
    // heap references are removed from the heap (the Rust side forgets them
    // rather than queuing a drop) and their objects handed to the caller.
    window.jsHeap.setTakingOwnership(true);
    let result: any;
    try {
      result = typeFromBytes(returnTypeDef).decode(decoder);
    } finally {
      window.jsHeap.setTakingOwnership(false);
    }
    // `&mut [T]` write-backs follow the return value, in argument order.
    copyBackMutArrays(decoder, argTypes, args);
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
  const classRegistry = (window as any).__wryClassRegistry;
  const ClassConstructor = classRegistry?.[className] ?? (window as any)[className];
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
            callExport(`${className}::__drop`, [U32_TYPE_DEF], UNDEFINED_TYPE_DEF, [handle]);
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
  exportRegistry.register(proxy, { drops: [[className, handle]] });

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
