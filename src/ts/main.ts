import { JSHeap } from "./heap.ts";
import "./ipc.ts";
import { DataDecoder, DataEncoder } from "./encoding.ts";
import { evaluate_from_rust_binary } from "./ipc.ts";
import { createWrapperFunction, BoolType, HeapRefType, NullType, U8Type, U16Type, U32Type, U64Type, OptionType, CallbackType } from "./types.ts";

/**
 * Function registry - maps function IDs to their serialization/deserialization specs.
 *
 * Each function has:
 * - Argument deserialization: how to read args from decoder
 * - Return serialization: how to write return value to encoder
 */
type FunctionSpec = (decoder: DataDecoder, encoder: DataEncoder) => void;

let functionRegistry: FunctionSpec[] = [];
window.setFunctionRegistry = (registry: FunctionSpec[]) => {
  functionRegistry = registry;
};
window.evaluate_from_rust_binary = evaluate_from_rust_binary;
window.jsHeap = new JSHeap();
window.createWrapperFunction = createWrapperFunction;
window.BoolType = BoolType;
window.HeapRefType = HeapRefType;
window.NullType = NullType;
window.OptionType = OptionType;
window.CallbackType = CallbackType;
window.U8Type = U8Type;
window.U16Type = U16Type;
window.U32Type = U32Type;
window.U64Type = U64Type;

declare global {
  interface Window {
    functionRegistry: FunctionSpec[];
    setFunctionRegistry: (registry: FunctionSpec[]) => void;
    evaluate_from_rust_binary: (fnId: number, dataBase64: string) => unknown;
    jsHeap: JSHeap;
    createWrapperFunction: typeof createWrapperFunction;
    BoolType: typeof BoolType;
    HeapRefType: typeof HeapRefType;
    NullType: typeof NullType;
    OptionType: typeof OptionType;
    CallbackType: typeof CallbackType;
    U8Type: typeof U8Type;
    U16Type: typeof U16Type;
    U32Type: typeof U32Type;
    U64Type: typeof U64Type;
  }
}
