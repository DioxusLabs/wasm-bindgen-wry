const wasm = new Proxy({}, { get: (_t, n) => window[n] });
const assert = new Proxy(function(){}, { get: (_t, n) => globalThis.__wbgAssert[n], apply: (_t, _s, a) => globalThis.__wbgAssert(...a) });

export const error_new = function(message) {
    return new Error(message)
}

export const call_ok = function() {
    assert.doesNotThrow(() => {
        let five = wasm.return_my_ok();
        assert.strictEqual(five, 5);
    })
}

export const call_err = function() {
    assert.throws(() => wasm.return_my_err(), {
        message: "MyError::Variant"
    });
}

function check_inflight(struct) {
    assert.strictEqual(struct.is_inflight(), false);
}

export const all_struct_methods = function() {
    let struct;
    assert.throws(() => wasm.Struct.new_err(), {
        message: "MyError::Variant"
    });
    assert.doesNotThrow(() => {
        struct = wasm.Struct.new();
    });
    check_inflight(struct);
    assert.doesNotThrow(() => {
        let five = struct.return_ok();
        assert.strictEqual(five, 5);
    });
    check_inflight(struct);
    assert.throws(() => struct.return_err(), {
        message: "MyError::Variant"
    });
    check_inflight(struct);
}

export const call_return_string = function() {
    assert.doesNotThrow(() => {
        let ok = wasm.return_string();
        assert.strictEqual(ok, "string here");
    })
}

export const call_jsvalue_ok = function() {
    assert.doesNotThrow(() => {
        let five = wasm.return_jsvalue_ok();
        assert.strictEqual(five, 5);
    })
}

export const call_jsvalue_err = function() {
    try {
        wasm.return_jsvalue_err();
        assert.fail("should have thrown");
    } catch (e) {
        assert.strictEqual(e, -1);
    }
}

export const call_string_ok = function() {
    assert.doesNotThrow(() => {
        let ok = wasm.return_string_ok();
        assert.strictEqual(ok, "Ok");
    })
}

export const call_string_err = function() {
    // the behaviour of Result<String, _> is so finicky that it's not obvious
    // how to reproduce reliably but also pass the test suite.
    assert.throws(() => e = wasm.return_string_err(), e => {
        // one thing we can do (uncomment to test)
        // is to throw an error in here.
        // throw new Error("should not cause a SIGBUS in Node")
        return e === "Er";
    });
}

export const call_enum_ok = function() {
    assert.doesNotThrow(() => {
        let ok = wasm.return_enum_ok();
        assert.strictEqual(ok, 2);
    })
}

export const call_enum_err = function() {
    assert.throws(() => {
        wasm.return_enum_err();
    }, {
        message: "MyError::Variant"
    })
}

export const call_unit = function() {
    assert.doesNotThrow(() => {
        wasm.return_unit_ok();
    });
    assert.throws(() => {
        wasm.return_unit_err();
    }, {
        message: "MyError::Variant"
    });
}

export const call_option = function() {
    assert.doesNotThrow(() => {
        let o = wasm.return_option_ok_some();
        assert.strictEqual(o, 10.0);
    });
    assert.doesNotThrow(() => {
        let o = wasm.return_option_ok_none();
        assert.strictEqual(o, undefined);
    });
    assert.throws(() => {
        wasm.return_option_err();
    }, {
        message: "MyError::Variant"
    });
}
