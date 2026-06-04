import { JSHeap } from "./heap.ts";
import "./ipc.ts";
import { acquire_handler_lock } from "./ipc.ts";
import { RawJsFunction, setFunctionRegistry } from "./function_registry.ts";
import { rustExports } from "./rust_exports.ts";

window.setFunctionRegistry = setFunctionRegistry;
window.__wry_acquire_handler_lock = acquire_handler_lock;
window.jsHeap = new JSHeap();
window.rustExports = rustExports;

declare global {
  interface Window {
    setFunctionRegistry: (registry: RawJsFunction[]) => void;
    __wry_acquire_handler_lock: () => unknown;
    jsHeap: JSHeap;
    rustExports: typeof rustExports;
  }
}
