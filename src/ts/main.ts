import { JSHeap } from "./heap.ts";
import "./ipc.ts";
import { evaluate_from_rust_binary } from "./ipc.ts";
import { createWrapperFunction, BoolType, HeapRefType, NullType, U8Type, U16Type, U32Type, U64Type, I8Type, I16Type, I32Type, I64Type, UsizeType, IsizeType, OptionType, CallbackType, F32Type, F64Type, ResultType, StringEnumType, strType } from "./types.ts";
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
window.I8Type = I8Type;
window.I16Type = I16Type;
window.I32Type = I32Type;
window.I64Type = I64Type;
window.UsizeType = UsizeType;
window.IsizeType = IsizeType;
window.F32Type = F32Type;
window.F64Type = F64Type;
window.StringEnumType = StringEnumType;
window.strType = strType;

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
    I8Type: typeof I8Type;
    I16Type: typeof I16Type;
    I32Type: typeof I32Type;
    I64Type: typeof I64Type;
    UsizeType: typeof UsizeType;
    IsizeType: typeof IsizeType;
    F32Type: typeof F32Type;
    F64Type: typeof F64Type;
    StringEnumType: typeof StringEnumType;
    strType: typeof strType;
  }
}
