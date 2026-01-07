import { JSHeap } from "./heap.ts";
import "./ipc.ts";
import { evaluate_from_rust_binary } from "./ipc.ts";
import { RawJsFunction, setFunctionRegistry } from "./function_registry.ts";
import { rustExports } from "./rust_exports.ts";

window.setFunctionRegistry = setFunctionRegistry;
window.evaluate_from_rust_binary = evaluate_from_rust_binary;
window.jsHeap = new JSHeap();
window.rustExports = rustExports;

declare global {
  interface Window {
    setFunctionRegistry: (registry: RawJsFunction[]) => void;
    evaluate_from_rust_binary: (dataBase64: string) => unknown;
    jsHeap: JSHeap;
    rustExports: typeof rustExports;
  }
}
