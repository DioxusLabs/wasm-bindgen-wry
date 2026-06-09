const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });
const wasm = new Proxy({}, { get: (_t, n) => window[n] });

export const works_call = a => {
    a();
};

export const works_thread = a => a(2);

let CANNOT_REUSE_CACHE = null;

export const cannot_reuse_call = a => {
    CANNOT_REUSE_CACHE = a;
};

export const cannot_reuse_call_again = () => {
    CANNOT_REUSE_CACHE();
};

export const long_lived_call1 = a => {
    a();
};

export const long_lived_call2 = a => a(2);

export const many_arity_call1 = a => {
    a();
};
export const many_arity_call2 = a => {
    a(1);
};
export const many_arity_call3 = a => {
    a(1, 2);
};
export const many_arity_call4 = a => {
    a(1, 2, 3);
};
export const many_arity_call5 = a => {
    a(1, 2, 3, 4);
};
export const many_arity_call6 = a => {
    a(1, 2, 3, 4, 5);
};
export const many_arity_call7 = a => {
    a(1, 2, 3, 4, 5, 6);
};
export const many_arity_call8 = a => {
    a(1, 2, 3, 4, 5, 6, 7);
};
export const many_arity_call9 = a => {
    a(1, 2, 3, 4, 5, 6, 7, 8);
};

export const option_call1 = a => {
    if (a) {
        a();
    }
};
export const option_call2 = a => {
    if (a) {
        return a(2);
    }
};
export const option_call3 = a => a == undefined;

let LONG_LIVED_DROPPING_CACHE = null;

export const long_lived_dropping_cache = a => {
    LONG_LIVED_DROPPING_CACHE = a;
};
export const long_lived_dropping_call = () => {
    LONG_LIVED_DROPPING_CACHE();
};

let LONG_LIVED_OPTION_DROPPING_CACHE = null;

export const long_lived_option_dropping_cache = a => {
    if (a) {
        LONG_LIVED_OPTION_DROPPING_CACHE = a;
        return true;
    } else {
        return false;
    }
}
export const long_lived_option_dropping_call = () => {
    LONG_LIVED_OPTION_DROPPING_CACHE();
}

let LONG_FNMUT_RECURSIVE_CACHE = null;

export const long_fnmut_recursive_cache = a => {
    LONG_FNMUT_RECURSIVE_CACHE = a;
};
export const long_fnmut_recursive_call = () => {
    LONG_FNMUT_RECURSIVE_CACHE();
};

export const fnmut_call = a => {
    a();
};

export const fnmut_thread = a => a(2);

let FNMUT_BAD_F = null;

export const fnmut_bad_call = a => {
    FNMUT_BAD_F = a;
    a();
};

export const fnmut_bad_again = x => {
    if (x) {
        FNMUT_BAD_F();
    }
};

export const string_arguments_call = a => {
    a('foo');
};

export const string_ret_call = a => {
    assert.strictEqual(a('foo'), 'foobar');
};

let DROP_DURING_CALL = null;
export const drop_during_call_save = f => {
  DROP_DURING_CALL = f;
};
export const drop_during_call_call = () => DROP_DURING_CALL();

export const js_test_closure_returner = () => {
  wasm.closure_returner().someKey();
};

export const calling_it_throws = a => {
  try {
    a();
    return false;
  } catch(_) {
    return true;
  }
};

export const call_val = f => f();

export const pass_reference_first_arg_twice = (a, b, c) => {
  b(a);
  c(a);
  a.free();
};

export const call_destroyed = f => {
  assert.throws(f, /closure invoked.*after being dropped/);
};

let FORGOTTEN_CLOSURE = null;

export const js_store_forgotten_closure = f => {
  FORGOTTEN_CLOSURE = f;
};

export const js_call_forgotten_closure = () => {
  FORGOTTEN_CLOSURE();
};

// Test for RefClosure - closure works during callback, throws after
let CLOSURE_WITH_CACHE = null;

export const closure_with_call = f => {
  f();
};

// Same as closure_with_call but used to test RefClosure -> &Closure deref
export const closure_with_call_closure = f => {
  f();
};

export const closure_with_cache = f => {
  CLOSURE_WITH_CACHE = f;
};

export const closure_with_call_cached = () => {
  CLOSURE_WITH_CACHE();
};

// Test that calling a RefClosure closure after it's been invalidated throws
let CLOSURE_WITH_ARG_CACHE = null;

export const closure_with_call_and_cache = f => {
  CLOSURE_WITH_ARG_CACHE = f;
  f(1);
  f(2);
  f(3);
};

export const closure_with_call_cached_throws = () => {
  try {
    CLOSURE_WITH_ARG_CACHE(42);
    return false; // Should not reach here
  } catch (e) {
    // Expected: closure invoked after being dropped
    return true;
  }
};

// Test for passing Closure by value (ownership transfer)
let OWNED_CLOSURE_CACHE = null;

export const closure_take_ownership = f => {
  // Store the closure and call it
  OWNED_CLOSURE_CACHE = f;
  f();
};

export const closure_take_ownership_with_arg = (f, value) => {
  f(value);
};

export const closure_call_stored = () => {
  // Call the previously stored closure
  OWNED_CLOSURE_CACHE();
};

// Test for ScopedClosure::borrow with Fn closures
export const closure_fn_with_call = f => {
  f();
};

export const closure_fn_with_call_arg = (f, value) => {
  f(value);
};

// Test for direct &dyn Fn/&mut dyn FnMut closures
export const immediate_closure_call = f => {
  f();
};

export const immediate_closure_call_arg = (f, value) => {
  f(value);
};

export const immediate_closure_call_ret = (f, value) => {
  return f(value);
};

export const immediate_closure_fn_call = f => {
  f();
};

export const immediate_closure_catches_panic = f => {
  try {
    f();
    return false;
  } catch (e) {
    return true;
  }
};

// Calls the closure, which may call immediate_closure_fnmut_reentrant_invoke
// to trigger reentrancy
let IMMEDIATE_REENTRANT_CB = null;
export const immediate_closure_fnmut_reentrant = f => {
  IMMEDIATE_REENTRANT_CB = f;
  f();
  IMMEDIATE_REENTRANT_CB = null;
};

// Called from inside the closure to attempt reentrant invocation
export const immediate_closure_fnmut_reentrant_invoke = () => {
  IMMEDIATE_REENTRANT_CB();
};

// Same pattern for Fn (immutable) closures
let IMMEDIATE_FN_REENTRANT_CB = null;
export const immediate_closure_fn_reentrant = f => {
  IMMEDIATE_FN_REENTRANT_CB = f;
  f();
  IMMEDIATE_FN_REENTRANT_CB = null;
};

export const immediate_closure_fn_reentrant_invoke = () => {
  IMMEDIATE_FN_REENTRANT_CB();
};
