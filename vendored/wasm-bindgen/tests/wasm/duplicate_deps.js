const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

let next = null;

export const assert_next_undefined = function() {
  next = undefined;
};

export const assert_next_ten = function() {
  next = 10;
};

export const foo = function(a) {
  console.log(a, next);
  assert.strictEqual(a, next);
  next = null;
};
