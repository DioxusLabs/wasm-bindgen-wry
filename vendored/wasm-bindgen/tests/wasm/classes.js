const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const js_simple = () => {
    const r = new wasm.ClassesSimple();
    assert.strictEqual(r.add(0), 0);
    assert.strictEqual(r.add(1), 1);
    assert.strictEqual(r.add(1), 2);
    r.add(2);
    assert.strictEqual(r.consume(), 4);
    assert.throws(() => r.free(), /null pointer passed to rust/);

    const r2 = wasm.ClassesSimple.with_contents(10);
    assert.strictEqual(r2.add(1), 11);
    assert.strictEqual(r2.add(2), 13);
    assert.strictEqual(r2.add(3), 16);
    r2.free();

    const r3 = new wasm.ClassesSimple();
    assert.strictEqual(r3.add(42), 42);
    r3.free();
};

export const js_strings = () => {
    const r = wasm.ClassesStrings1.new();
    r.set(3);
    let bar = r.bar('baz');
    r.free();
    assert.strictEqual(bar.name(), 'foo-baz-3');
    bar.free();
};

export const js_exceptions = (is_panic_unwind) => {
    // this test only works when `--debug` is passed to `wasm-bindgen` (or the
    // equivalent thereof). `process` is a nodejs global with no wry analogue, so
    // the debug gate reads as unset here and the debug checks apply.
    if (globalThis.process?.env?.WASM_BINDGEN_NO_DEBUG)
        return;
    assert.throws(() => new wasm.ClassesExceptions1(), /cannot invoke `new` directly/);
    let a = wasm.ClassesExceptions1.new();
    a.free();
    assert.throws(() => a.free(), /null pointer passed to rust/);

    let b = wasm.ClassesExceptions1.new();
    b.foo(b);
    assert.throws(() => b.bar(b), /recursive use of an object/);
    // wasm-bindgen-implementation-specific: on wasm a failed `borrow_mut`'s
    // `throw_str` leaves a dangling `RefMut`, so a later `free()` throws
    // "attempted to take ownership". wry unwinds the failed borrow cleanly (the
    // borrow guard reinserts the object), so `b` stays usable; the broken-state
    // assertion has no native analogue. `free()` still cleans `b` up.
    b.free();

    let c = wasm.ClassesExceptions1.new();
    let d = wasm.ClassesExceptions2.new();
    assert.throws(() => c.foo(d), /expected instance of ClassesExceptions1/);
    d.free();
    c.free();
};

export const js_pass_one_to_another = () => {
    let a = wasm.ClassesPassA.new();
    let b = wasm.ClassesPassB.new();
    a.foo(b);
    a.bar(b);
    a.free();
};

export const take_class = foo => {
    assert.strictEqual(foo.inner(), 13);
    foo.free();
    assert.throws(() => foo.free(), /null pointer passed to rust/);
};

export const js_constructors = () => {
    const foo = new wasm.ConstructorsFoo(1);
    assert.strictEqual(foo.get_number(), 1);
    foo.free();

    assert.strictEqual(wasm.ConstructorsBar.new, undefined);
    const foo2 = new wasm.ConstructorsFoo(2);
    assert.strictEqual(foo2.get_number(), 2);
    foo2.free();

    const bar = new wasm.ConstructorsBar(3, 4);
    assert.strictEqual(bar.get_sum(), 7);
    bar.free();

    assert.strictEqual(wasm.ConstructorsBar.other_name, undefined);
    const bar2 = new wasm.ConstructorsBar(5, 6);
    assert.strictEqual(bar2.get_sum(), 11);
    bar2.free();

    assert.strictEqual(wasm.cross_item_construction().get_sum(), 15);
};

export const js_empty_structs = () => {
    wasm.OtherEmpty.return_a_value();
};

export const js_public_fields = () => {
    const a = wasm.PublicFields.new();
    assert.strictEqual(a.a, 0);
    a.a = 3;
    assert.strictEqual(a.a, 3);

    assert.strictEqual(a.b, 0);
    a.b = 7;
    assert.strictEqual(a.b, 7);

    assert.strictEqual(a.c, 0);
    a.c = 8;
    assert.strictEqual(a.c, 8);

    assert.strictEqual(a.d, 0);
    a.d = 3.3;
    assert.strictEqual(a.d, 3);

    assert.strictEqual(a.skipped, undefined);
};

export const js_getter_with_clone = () => {
    const a = wasm.GetterWithCloneStruct.new();
    assert.strictEqual(a.a, '');
    a.a = 'foo';
    assert.strictEqual(a.a, 'foo');

    const b = wasm.GetterWithCloneStructField.new();
    assert.strictEqual(b.a, '');
    b.a = 'foo';
    assert.strictEqual(b.a, 'foo');
};

export const js_using_self = () => {
    wasm.UseSelf.new().free();
};

export const js_readonly_fields = () => {
    const a = wasm.Readonly.new();
    assert.strictEqual(a.a, 0);
    a.a = 3;
    assert.strictEqual(a.a, 0);
    a.free();
};

export const js_double_consume = () => {
    const r = new wasm.DoubleConsume();
    assert.throws(() => r.consume(r));
};


export const js_js_rename = () => {
    (new wasm.JsRename()).bar();
    wasm.classes_foo();
};

export const js_access_fields = () => {
    assert.ok((new wasm.AccessFieldFoo()).bar instanceof wasm.AccessFieldBar);
    assert.ok((new wasm.AccessField0())[0] instanceof wasm.AccessFieldBar);
};

export const js_renamed_export = () => {
    const x = new wasm.JsRenamedExport();
    assert.ok(x.x === 3);
    x.foo();
    x.bar(x);
};

export const js_renamed_field = () => {
    const x = new wasm.RenamedField();
    assert.ok(x.bar === 3);

    x.foo();
}

export const js_conditional_skip = () => {
    // wasm-specific: `ConditionalSkip` is `#[cfg_attr(target_family="wasm", wasm_bindgen(...))]`,
    // so on the native wry target the struct gets no `#[wasm_bindgen]` and is never
    // exported as a JS class; N/A here.
}

export const js_conditional_bindings = () => {
    // wasm-specific: `ConditionalBindings` is `#[cfg_attr(target_family="wasm", wasm_bindgen)]`,
    // so on the native wry target it is never exported as a JS class; N/A here.
};

export const js_assert_none = x => {
  assert.strictEqual(x, undefined);
};
export const js_assert_some = x => {
  assert.ok(x instanceof wasm.OptionClass);
};
export const js_return_none1 = () => null;
export const js_return_none2 = () => undefined;
export const js_return_some = x => x;

export const js_test_option_classes = () => {
  assert.strictEqual(wasm.option_class_none(), undefined);
  wasm.option_class_assert_none(undefined);
  wasm.option_class_assert_none(null);
  const c = wasm.option_class_some();
  assert.ok(c instanceof wasm.OptionClass);
  wasm.option_class_assert_some(c);
};

export const js_test_inspectable_classes = () => {
    const inspectable = wasm.Inspectable.new();
    const not_inspectable = wasm.NotInspectable.new();
    // Inspectable classes have a toJSON and toString implementation generated
    assert.deepStrictEqual(inspectable.toJSON(), { a: inspectable.a });
    assert.strictEqual(inspectable.toString(), `{"a":${inspectable.a}}`);
    // nodejs-specific: the `console.log`-formatting assertions use nodejs
    // `process.stdout`/`console.Console`; N/A on the native wry target.
    // Non-inspectable classes do not have a toJSON or toString generated
    assert.strictEqual(not_inspectable.toJSON, undefined);
    assert.strictEqual(not_inspectable.toString(), '[object Object]');
    inspectable.free();
    not_inspectable.free();
};

export const js_test_inspectable_classes_can_override_generated_methods = () => {
    const overridden_inspectable = wasm.OverriddenInspectable.new();
    // Inspectable classes can have the generated toJSON and toString overwritten
    assert.strictEqual(overridden_inspectable.a, 0);
    assert.deepStrictEqual(overridden_inspectable.toJSON(), 'JSON was overwritten');
    assert.strictEqual(overridden_inspectable.toString(), 'string was overwritten');
    overridden_inspectable.free();
};

export const js_test_class_defined_in_macro = () => {
    const macroClass = new wasm.InsideMacro();
    assert.strictEqual(macroClass.a, 3);
    macroClass.a = 5;
    assert.strictEqual(macroClass.a, 5);
};

export const js_classless_this = () => {
    const obj1 = { number: 42 };
    const result1 = wasm.classless_this_get_number.call(obj1);
    assert.strictEqual(result1, 42);

    const obj2 = { count: 10 };
    const result2 = wasm.classless_this_add.call(obj2, 5);
    assert.strictEqual(result2, 15);

    const result3 = wasm.classless_this_add.apply(obj2, [7]);
    assert.strictEqual(result3, 17);

    const obj3 = { test: 'value' };
    const result4 = wasm.classless_this_consume_jsvalue.call(obj3);
    assert.strictEqual(result4, true);
};
