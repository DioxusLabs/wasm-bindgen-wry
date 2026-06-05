const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const PI = 3.14159;

export const add = function add(a, b) {
    return a + b;
};

export const multiply = function multiply(a, b) {
    return a * b;
};
