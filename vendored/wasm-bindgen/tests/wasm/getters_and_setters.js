const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const _1_js = (rules) => {
    assert.equal(rules.field, 1);
    rules.field *= 2;
    return rules;
}

export const _2_js = (rules) => {
    let value = rules.no_js_name__no_getter_with_name__no_getter_without_name();
    assert.equal(value, 2);
    rules.set_no_js_name__no_setter_with_name__no_setter_without_name(value * 2);
    return rules;
}

export const _3_js = (rules) => {
    let value = rules.no_js_name__no_getter_with_name__getter_without_name;
    assert.equal(value, 3);
    rules.no_js_name__no_setter_with_name__setter_without_name = value * 2;
    return rules;
}

export const _4_js = (rules) => {
    let value = rules.new_no_js_name__getter_with_name__getter_without_name;
    assert.equal(value, 4);
    rules.new_no_js_name__setter_with_name__setter_without_name = value * 2;
    return rules;
}

export const _5_js = (rules) => {
    let value = rules.new_js_name__no_getter_with_name__no_getter_without_name();
    assert.equal(value, 5);
    rules.new_js_name__no_setter_with_name__no_setter_without_name(value * 2);
    return rules;
}

export const _6_js = (rules) => {
    let value = rules.new_js_name__no_getter_with_name__getter_without_name;
    assert.equal(value, 6);
    rules.new_js_name__no_setter_with_name__setter_without_name = value * 2;
    return rules;
}

export const _7_js = (rules) => {
    let value = rules.new_js_name__getter_with_name__no_getter_without_name_for_field;
    assert.equal(value, 7);
    rules.new_js_name__setter_with_name__no_setter_without_name_for_field = value * 2;
    return rules;
}

export const _8_js = (rules) => {
    let value = rules.new_js_name__no_getter_setter_with_name__getter_setter_without_name__same_getter_setter_name;
    assert.equal(value, 8);
    rules.new_js_name__no_getter_setter_with_name__getter_setter_without_name__same_getter_setter_name = value * 2;
    return rules;
}

export const _9_js = (rules) => {
    let value = rules.new_js_name__no_getter_setter_with_name__getter_setter_without_name__same_getter_setter_name__same_getter_setter_origin_name;
    assert.equal(value, 9);
    rules.new_js_name__no_getter_setter_with_name__getter_setter_without_name__same_getter_setter_name__same_getter_setter_origin_name = value * 2;
    return rules;
}

export const _10_js = (rules) => {
    let value = rules.new_js_name__getter_setter_with_name__no_getter_setter_without_name_for_field__same_getter_setter_name;
    assert.equal(value, 10);
    rules.new_js_name__getter_setter_with_name__no_getter_setter_without_name_for_field__same_getter_setter_name = value * 2;
    return rules;
}

export const _11_js = (rules) => {
    let value = rules.new_js_name__getter_with_name__no_getter_without_name_for_field__same_getter_setter_name;
    assert.equal(value, 11);
    rules.new_js_name__setter_with_name__no_setter_without_name_for_field__same_getter_setter_name = value * 2;
    return rules;
}

export const _12_js = (rules) => {
    let value = rules.new_js_name__getter_setter_with_name__no_getter_setter_without_name_for_field__same_getter_setter_name__same_getter_setter_origin_name;
    assert.equal(value, 12);
    rules.new_js_name__getter_setter_with_name__no_getter_setter_without_name_for_field__same_getter_setter_name__same_getter_setter_origin_name = value * 2;
    return rules;
}

export const _13_js = (rules) => {
    let value = rules.new_js_name__getter_with_name__no_getter_without_name_for_field__same_getter_setter_name__same_getter_setter_origin_name;
    assert.equal(value, 13);
    rules.new_js_name__setter_with_name__no_setter_without_name_for_field__same_getter_setter_name__same_getter_setter_origin_name = value * 2;
    return rules;
}

export const test_getter_compute = x => {
  assert.equal(x.foo, 3)
};

export const test_setter_compute = x => {
  x.foo = 97;
};

export const test_statics = x => {
    assert.equal(x.field, 3);
    assert.equal(wasm.Statics.field, 4);
    x.field = 13;
    wasm.Statics.field = 14;
}
