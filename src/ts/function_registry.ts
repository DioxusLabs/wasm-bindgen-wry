import { DataDecoder, DataEncoder } from "./encoding";

/**
 * Function registry - maps function IDs to their serialization/deserialization specs.
 *
 * Each function has:
 * - Argument deserialization: how to read args from decoder
 * - Return serialization: how to write return value to encoder
 */
export type FunctionSpec = (decoder: DataDecoder, encoder: DataEncoder) => void;

let functionRegistry: FunctionSpec[] | null = null;

export function getFunctionRegistry(): FunctionSpec[] {
  return functionRegistry!;
}

export function setFunctionRegistry(registry: FunctionSpec[]) {
  functionRegistry = registry;
}
