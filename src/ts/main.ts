import { JSHeap } from "./heap.ts";
import "./ipc.ts";
import { evaluate_from_rust_binary } from "./ipc.ts";
import { createWrapperFunction, BoolType, HeapRefType, NullType, U8Type, U16Type, U32Type, U64Type, OptionType, CallbackType, F32Type, F64Type, ResultType } from "./types.ts";
import { FunctionSpec, setFunctionRegistry } from "./function_registry.ts";


window.setFunctionRegistry = setFunctionRegistry;
window.evaluate_from_rust_binary = evaluate_from_rust_binary;
window.jsHeap = new JSHeap();
window.createWrapperFunction = createWrapperFunction;
window.BoolType = BoolType;
window.HeapRefType = HeapRefType;
window.NullType = NullType;
window.OptionType = OptionType;
window.CallbackType = CallbackType;
window.ResultType = ResultType;
window.U8Type = U8Type;
window.U16Type = U16Type;
window.U32Type = U32Type;
window.U64Type = U64Type;
window.F32Type = F32Type;
window.F64Type = F64Type;

declare global {
  interface Window {
    setFunctionRegistry: (registry: FunctionSpec[]) => void;
    evaluate_from_rust_binary: (dataBase64: string) => unknown;
    jsHeap: JSHeap;
    createWrapperFunction: typeof createWrapperFunction;
    BoolType: typeof BoolType;
    HeapRefType: typeof HeapRefType;
    NullType: typeof NullType;
    OptionType: typeof OptionType;
    CallbackType: typeof CallbackType;
    ResultType: typeof ResultType;
    U8Type: typeof U8Type;
    U16Type: typeof U16Type;
    U32Type: typeof U32Type;
    U64Type: typeof U64Type;
    F32Type: typeof F32Type;
    F64Type: typeof F64Type;
  }
}
