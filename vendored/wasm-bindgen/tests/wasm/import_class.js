const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });
const wasm = new Proxy({}, { get: (_t, n) => window[n] });

export const math_log = Math.log;

export const StaticFunction = class {
  static bar() { return 2; }
};

class Construct {
  static create() {
    const ret = new Construct();
    ret.internal_string = 'this';
    return ret;
  }

  get_internal_string() {
    return this.internal_string;
  }

  append_to_internal_string(s) {
    this.internal_string += s;
  }

  assert_internal_string(s) {
    assert.strictEqual(this.internal_string, s);
  }

  ["kebab-case"]() {
    return 42;
  }
  
  get ["kebab-case-val"]() {
    return 42;
  }

  set ["kebab-case-val"](val) {}

  static ["static-kebab-case"]() {
    return 42;
  }

  static get ["static-kebab-case-val"]() {
    return 42;
  }

  static set ["static-kebab-case-val"](val) {}
}

Construct.internal_string = '';
export { Construct };

class NewConstructor {
  constructor(field) {
    this.field = field;
  }

  get() {
    return this.field + 1;
  }
}

export const NewConstructors = NewConstructor;
export const default = NewConstructor;

let switch_called = false;
class SwitchMethods {
  constructor() {
  }

  static a() {
    switch_called = true;
  }

  b() {
    switch_called = true;
  }
}
export { SwitchMethods };
export const switch_methods_called = function() {
  const tmp = switch_called;
  switch_called = false;
  return tmp;
};
export const switch_methods_a = function() { SwitchMethods.a = function() {}; };
export const switch_methods_b = function() { SwitchMethods.prototype.b = function() {}; };

export const Properties = class {
  constructor() {
    this.num = 1;
  }

  get a() {
    return this.num;
  }

  set a(val) {
    this.num = val;
  }
};

export const RenameProperties = class {
  constructor() {
    this.num = 1;
  }

  get a() {
    return this.num;
  }

  set a(val) {
    this.num = val;
  }
};

class Options {
}
export { Options };

export const take_none = function(val) {
  assert.strictEqual(val, undefined);
};

export const take_some = function(val) {
  assert.strictEqual(val === undefined, false);
};

export const return_null = function() {
  return null;
};

export const return_undefined = function() {
  return undefined;
};

export const return_some = function() {
  return new Options();
};

export const run_rust_option_tests = function() {
  wasm.rust_take_none();
  wasm.rust_take_none(null)
  wasm.rust_take_none(undefined);
  wasm.rust_take_some(new Options());
  assert.strictEqual(wasm.rust_return_none(), undefined);
  assert.strictEqual(wasm.rust_return_none(), undefined);
  assert.strictEqual(wasm.rust_return_some() === undefined, false);
};

export const CatchConstructors = class {
  constructor(x) {
    if (x == 0) {
      throw new Error('bad!');
    }
  }
};

export const StaticStructural = class {
  static static_structural(x) {
    return x + 3;
  }
};

class InnerClass {
  static inner_static_function(x) {
    return x + 5;
  }

  static create_inner_instance() {
    const ret = new InnerClass();
    ret.internal_int = 3;
    return ret;
  }

  get_internal_int() {
    return this.internal_int;
  }

  append_to_internal_int(i) {
    this.internal_int += i;
  }

  assert_internal_int(i) {
    assert.strictEqual(this.internal_int, i);
  }
}

export const nestedNamespace = {
  InnerClass: InnerClass
}
