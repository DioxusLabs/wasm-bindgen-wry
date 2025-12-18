import { DataDecoder, DataEncoder } from "./encoding";

/**
 * Function registry - maps function IDs to their serialization/deserialization specs.
 *
 * Each function has:
 * - Argument deserialization: how to read args from decoder
 * - Return serialization: how to write return value to encoder
 */
export type FunctionSpec = (decoder: DataDecoder, encoder: DataEncoder) => void;

let queuedForFunctionRegistryInitialization: (() => void)[] = [];
let functionRegistry: FunctionSpec[] | null = null;

 function runWithFunctionRegistryInitialized(fn: () => void) {
  if (functionRegistry) {
    fn();
  } else {
    queuedForFunctionRegistryInitialization.push(fn);
  }
}

export async function getFunctionRegistry(): Promise<FunctionSpec[]> {
  return new Promise((resolve) => {
    runWithFunctionRegistryInitialized(() => {
      resolve(functionRegistry!);
    });
  });
}

export function setFunctionRegistry(registry: FunctionSpec[]) {
  functionRegistry = registry;
  for (const fn of queuedForFunctionRegistryInitialization) {
    fn();
  }
  queuedForFunctionRegistryInitialization = [];
}
