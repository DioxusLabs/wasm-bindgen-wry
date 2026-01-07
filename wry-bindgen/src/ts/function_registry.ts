import { TypeClass } from "./types";

/**
 * Function registry - maps function IDs to raw JS functions.
 * Type information is now passed at call time, not registration time.
 */
export type RawJsFunction = (...args: any[]) => any;

let functionRegistry: RawJsFunction[] | null = null;

/**
 * Type cache - maps type IDs to parsed type information.
 * Used for caching type definitions so they don't need to be re-parsed.
 */
export interface CachedTypeInfo {
  paramTypes: TypeClass[];
  returnType: TypeClass;
}

const typeCache: Map<number, CachedTypeInfo> = new Map();

export function getFunctionRegistry(): RawJsFunction[] {
  return functionRegistry!;
}

export function setFunctionRegistry(registry: RawJsFunction[]) {
  functionRegistry = registry;
}

export function getTypeCache(): Map<number, CachedTypeInfo> {
  return typeCache;
}
