let wasm;

const heap = new Array(128).fill(undefined);

heap.push(undefined, null, true, false);

function getObject(idx) { return heap[idx]; }

let heap_next = heap.length;

function dropObject(idx) {
    if (idx < 132) return;
    heap[idx] = heap_next;
    heap_next = idx;
}

function takeObject(idx) {
    const ret = getObject(idx);
    dropObject(idx);
    return ret;
}

function addHeapObject(obj) {
    if (heap_next === heap.length) heap.push(heap.length + 1);
    const idx = heap_next;
    heap_next = heap[idx];

    heap[idx] = obj;
    return idx;
}

const cachedTextDecoder = (typeof TextDecoder !== 'undefined' ? new TextDecoder('utf-8', { ignoreBOM: true, fatal: true }) : { decode: () => { throw Error('TextDecoder not available') } } );

if (typeof TextDecoder !== 'undefined') { cachedTextDecoder.decode(); };

let cachedUint8Memory0 = null;

function getUint8Memory0() {
    if (cachedUint8Memory0 === null || cachedUint8Memory0.byteLength === 0) {
        cachedUint8Memory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8Memory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return cachedTextDecoder.decode(getUint8Memory0().subarray(ptr, ptr + len));
}

let WASM_VECTOR_LEN = 0;

const cachedTextEncoder = (typeof TextEncoder !== 'undefined' ? new TextEncoder('utf-8') : { encode: () => { throw Error('TextEncoder not available') } } );

const encodeString = (typeof cachedTextEncoder.encodeInto === 'function'
    ? function (arg, view) {
    return cachedTextEncoder.encodeInto(arg, view);
}
    : function (arg, view) {
    const buf = cachedTextEncoder.encode(arg);
    view.set(buf);
    return {
        read: arg.length,
        written: buf.length
    };
});

function passStringToWasm0(arg, malloc, realloc) {

    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8Memory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8Memory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }

    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8Memory0().subarray(ptr + offset, ptr + len);
        const ret = encodeString(arg, view);

        offset += ret.written;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function isLikeNone(x) {
    return x === undefined || x === null;
}

let cachedInt32Memory0 = null;

function getInt32Memory0() {
    if (cachedInt32Memory0 === null || cachedInt32Memory0.byteLength === 0) {
        cachedInt32Memory0 = new Int32Array(wasm.memory.buffer);
    }
    return cachedInt32Memory0;
}

let cachedFloat64Memory0 = null;

function getFloat64Memory0() {
    if (cachedFloat64Memory0 === null || cachedFloat64Memory0.byteLength === 0) {
        cachedFloat64Memory0 = new Float64Array(wasm.memory.buffer);
    }
    return cachedFloat64Memory0;
}

let cachedBigInt64Memory0 = null;

function getBigInt64Memory0() {
    if (cachedBigInt64Memory0 === null || cachedBigInt64Memory0.byteLength === 0) {
        cachedBigInt64Memory0 = new BigInt64Array(wasm.memory.buffer);
    }
    return cachedBigInt64Memory0;
}

function debugString(val) {
    // primitive types
    const type = typeof val;
    if (type == 'number' || type == 'boolean' || val == null) {
        return  `${val}`;
    }
    if (type == 'string') {
        return `"${val}"`;
    }
    if (type == 'symbol') {
        const description = val.description;
        if (description == null) {
            return 'Symbol';
        } else {
            return `Symbol(${description})`;
        }
    }
    if (type == 'function') {
        const name = val.name;
        if (typeof name == 'string' && name.length > 0) {
            return `Function(${name})`;
        } else {
            return 'Function';
        }
    }
    // objects
    if (Array.isArray(val)) {
        const length = val.length;
        let debug = '[';
        if (length > 0) {
            debug += debugString(val[0]);
        }
        for(let i = 1; i < length; i++) {
            debug += ', ' + debugString(val[i]);
        }
        debug += ']';
        return debug;
    }
    // Test for built-in
    const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
    let className;
    if (builtInMatches.length > 1) {
        className = builtInMatches[1];
    } else {
        // Failed to match the standard '[object ClassName]'
        return toString.call(val);
    }
    if (className == 'Object') {
        // we're a user defined class or Object
        // JSON.stringify avoids problems with cycles, and is generally much
        // easier than looping through ownProperties of `val`.
        try {
            return 'Object(' + JSON.stringify(val) + ')';
        } catch (_) {
            return 'Object';
        }
    }
    // errors
    if (val instanceof Error) {
        return `${val.name}: ${val.message}\n${val.stack}`;
    }
    // TODO we could test for more things here, like `Set`s and `Map`s.
    return className;
}

const CLOSURE_DTORS = new FinalizationRegistry(state => {
    wasm.__wbindgen_export_2.get(state.dtor)(state.a, state.b)
});

function makeClosure(arg0, arg1, dtor, f) {
    const state = { a: arg0, b: arg1, cnt: 1, dtor };
    const real = (...args) => {
        // First up with a closure we increment the internal reference
        // count. This ensures that the Rust closure environment won't
        // be deallocated while we're invoking it.
        state.cnt++;
        try {
            return f(state.a, state.b, ...args);
        } finally {
            if (--state.cnt === 0) {
                wasm.__wbindgen_export_2.get(state.dtor)(state.a, state.b);
                state.a = 0;
                CLOSURE_DTORS.unregister(state)
            }
        }
    };
    real.original = state;
    CLOSURE_DTORS.register(real, state, state);
    return real;
}
function __wbg_adapter_54(arg0, arg1, arg2) {
    wasm.wasm_bindgen__convert__closures__invoke1__h36fd4c0a3ba5e194(arg0, arg1, addHeapObject(arg2));
}

function makeMutClosure(arg0, arg1, dtor, f) {
    const state = { a: arg0, b: arg1, cnt: 1, dtor };
    const real = (...args) => {
        // First up with a closure we increment the internal reference
        // count. This ensures that the Rust closure environment won't
        // be deallocated while we're invoking it.
        state.cnt++;
        const a = state.a;
        state.a = 0;
        try {
            return f(a, state.b, ...args);
        } finally {
            if (--state.cnt === 0) {
                wasm.__wbindgen_export_2.get(state.dtor)(a, state.b);
                CLOSURE_DTORS.unregister(state)
            } else {
                state.a = a;
            }
        }
    };
    real.original = state;
    CLOSURE_DTORS.register(real, state, state);
    return real;
}
function __wbg_adapter_57(arg0, arg1, arg2) {
    wasm.wasm_bindgen__convert__closures__invoke1_mut__h42e5efd229e85853(arg0, arg1, addHeapObject(arg2));
}

function __wbg_adapter_62(arg0, arg1) {
    wasm.wasm_bindgen__convert__closures__invoke0_mut__h11cf4b02932b0f84(arg0, arg1);
}

function __wbg_adapter_65(arg0, arg1) {
    wasm.wasm_bindgen__convert__closures__invoke0_mut__h5005ec47677e1ff2(arg0, arg1);
}

function __wbg_adapter_68(arg0, arg1, arg2) {
    wasm.wasm_bindgen__convert__closures__invoke1_mut__hf9c1ead8caf3d632(arg0, arg1, addHeapObject(arg2));
}

function __wbg_adapter_71(arg0, arg1) {
    wasm.wasm_bindgen__convert__closures__invoke0_mut__h78a621cb74d3c34e(arg0, arg1);
}

function __wbg_adapter_74(arg0, arg1, arg2) {
    wasm._dyn_core__ops__function__FnMut__A____Output___R_as_wasm_bindgen__closure__WasmClosure___describe__invoke__h9c9eb8db5fd31be6(arg0, arg1, addHeapObject(arg2));
}

function getCachedStringFromWasm0(ptr, len) {
    if (ptr === 0) {
        return getObject(len);
    } else {
        return getStringFromWasm0(ptr, len);
    }
}

let cachedUint32Memory0 = null;

function getUint32Memory0() {
    if (cachedUint32Memory0 === null || cachedUint32Memory0.byteLength === 0) {
        cachedUint32Memory0 = new Uint32Array(wasm.memory.buffer);
    }
    return cachedUint32Memory0;
}

function getArrayJsValueFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    const mem = getUint32Memory0();
    const slice = mem.subarray(ptr / 4, ptr / 4 + len);
    const result = [];
    for (let i = 0; i < slice.length; i++) {
        result.push(takeObject(slice[i]));
    }
    return result;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        wasm.__wbindgen_exn_store(addHeapObject(e));
    }
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);

            } catch (e) {
                if (module.headers.get('Content-Type') != 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else {
                    throw e;
                }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);

    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };

        } else {
            return instance;
        }
    }
}

function __wbg_get_imports() {
    const imports = {};
    imports.wbg = {};
    imports.wbg.__wbindgen_object_drop_ref = function(arg0) {
        takeObject(arg0);
    };
    imports.wbg.__wbindgen_object_clone_ref = function(arg0) {
        const ret = getObject(arg0);
        return addHeapObject(ret);
    };
    imports.wbg.__wbindgen_is_bigint = function(arg0) {
        const ret = typeof(getObject(arg0)) === 'bigint';
        return ret;
    };
    imports.wbg.__wbindgen_bigint_from_u64 = function(arg0) {
        const ret = BigInt.asUintN(64, arg0);
        return addHeapObject(ret);
    };
    imports.wbg.__wbindgen_jsval_eq = function(arg0, arg1) {
        const ret = getObject(arg0) === getObject(arg1);
        return ret;
    };
    imports.wbg.__wbindgen_error_new = function(arg0, arg1) {
        const ret = new Error(getStringFromWasm0(arg0, arg1));
        return addHeapObject(ret);
    };
    imports.wbg.__wbindgen_string_get = function(arg0, arg1) {
        const obj = getObject(arg1);
        const ret = typeof(obj) === 'string' ? obj : undefined;
        var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        var len1 = WASM_VECTOR_LEN;
        getInt32Memory0()[arg0 / 4 + 1] = len1;
        getInt32Memory0()[arg0 / 4 + 0] = ptr1;
    };
    imports.wbg.__wbindgen_is_object = function(arg0) {
        const val = getObject(arg0);
        const ret = typeof(val) === 'object' && val !== null;
        return ret;
    };
    imports.wbg.__wbindgen_is_undefined = function(arg0) {
        const ret = getObject(arg0) === undefined;
        return ret;
    };
    imports.wbg.__wbindgen_in = function(arg0, arg1) {
        const ret = getObject(arg0) in getObject(arg1);
        return ret;
    };
    imports.wbg.__wbindgen_string_new = function(arg0, arg1) {
        const ret = getStringFromWasm0(arg0, arg1);
        return addHeapObject(ret);
    };
    imports.wbg.__wbindgen_cb_drop = function(arg0) {
        const obj = takeObject(arg0).original;
        if (obj.cnt-- == 1) {
            obj.a = 0;
            return true;
        }
        const ret = false;
        return ret;
    };
    imports.wbg.__wbg_new_abda76e883ba8a5f = function() {
        const ret = new Error();
        return addHeapObject(ret);
    };
    imports.wbg.__wbg_stack_658279fe44541cf6 = function(arg0, arg1) {
        const ret = getObject(arg1).stack;
        const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        getInt32Memory0()[arg0 / 4 + 1] = len1;
        getInt32Memory0()[arg0 / 4 + 0] = ptr1;
    };
    imports.wbg.__wbg_error_f851667af71bcfc6 = function(arg0, arg1) {
        var v0 = getCachedStringFromWasm0(arg0, arg1);
    if (arg0 !== 0) { wasm.__wbindgen_free(arg0, arg1, 1); }
    console.error(v0);
};
imports.wbg.__wbindgen_is_string = function(arg0) {
    const ret = typeof(getObject(arg0)) === 'string';
    return ret;
};
imports.wbg.__wbg_debug_783a3d4910bc24c7 = function(arg0, arg1) {
    var v0 = getArrayJsValueFromWasm0(arg0, arg1).slice();
    wasm.__wbindgen_free(arg0, arg1 * 4, 4);
    console.debug(...v0);
};
imports.wbg.__wbg_error_71d6845bf00a930f = function(arg0, arg1) {
    var v0 = getArrayJsValueFromWasm0(arg0, arg1).slice();
    wasm.__wbindgen_free(arg0, arg1 * 4, 4);
    console.error(...v0);
};
imports.wbg.__wbg_log_1f7f93998ab961f7 = function(arg0, arg1) {
    var v0 = getArrayJsValueFromWasm0(arg0, arg1).slice();
    wasm.__wbindgen_free(arg0, arg1 * 4, 4);
    console.log(...v0);
};
imports.wbg.__wbg_warn_0b90a269a514ae1d = function(arg0, arg1) {
    var v0 = getArrayJsValueFromWasm0(arg0, arg1).slice();
    wasm.__wbindgen_free(arg0, arg1 * 4, 4);
    console.warn(...v0);
};
imports.wbg.__wbindgen_number_get = function(arg0, arg1) {
    const obj = getObject(arg1);
    const ret = typeof(obj) === 'number' ? obj : undefined;
    getFloat64Memory0()[arg0 / 8 + 1] = isLikeNone(ret) ? 0 : ret;
    getInt32Memory0()[arg0 / 4 + 0] = !isLikeNone(ret);
};
imports.wbg.__wbindgen_boolean_get = function(arg0) {
    const v = getObject(arg0);
    const ret = typeof(v) === 'boolean' ? (v ? 1 : 0) : 2;
    return ret;
};
imports.wbg.__wbindgen_is_null = function(arg0) {
    const ret = getObject(arg0) === null;
    return ret;
};
imports.wbg.__wbg_init_0dd800d675f746ca = function(arg0, arg1, arg2, arg3) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    const ret = echarts.init(getObject(arg0), v0, takeObject(arg3));
    return addHeapObject(ret);
};
imports.wbg.__wbg_setOption_c88e3b017b72bdcf = function(arg0, arg1) {
    getObject(arg0).setOption(takeObject(arg1));
};
imports.wbg.__wbg_on_9126a83b72674681 = function(arg0, arg1, arg2, arg3) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    getObject(arg0).on(v0, takeObject(arg3));
};
imports.wbg.__wbindgen_jsval_loose_eq = function(arg0, arg1) {
    const ret = getObject(arg0) == getObject(arg1);
    return ret;
};
imports.wbg.__wbindgen_as_number = function(arg0) {
    const ret = +getObject(arg0);
    return ret;
};
imports.wbg.__wbindgen_number_new = function(arg0) {
    const ret = arg0;
    return addHeapObject(ret);
};
imports.wbg.__wbindgen_bigint_from_i64 = function(arg0) {
    const ret = arg0;
    return addHeapObject(ret);
};
imports.wbg.__wbg_getwithrefkey_4a92a5eca60879b9 = function(arg0, arg1) {
    const ret = getObject(arg0)[getObject(arg1)];
    return addHeapObject(ret);
};
imports.wbg.__wbg_set_9182712abebf82ef = function(arg0, arg1, arg2) {
    getObject(arg0)[takeObject(arg1)] = takeObject(arg2);
};
imports.wbg.__wbindgen_is_falsy = function(arg0) {
    const ret = !getObject(arg0);
    return ret;
};
imports.wbg.__wbg_instanceof_Window_99dc9805eaa2614b = function(arg0) {
    let result;
    try {
        result = getObject(arg0) instanceof Window;
    } catch (_) {
        result = false;
    }
    const ret = result;
    return ret;
};
imports.wbg.__wbg_document_5257b70811e953c0 = function(arg0) {
    const ret = getObject(arg0).document;
    return isLikeNone(ret) ? 0 : addHeapObject(ret);
};
imports.wbg.__wbg_location_0f233324e8e8c699 = function(arg0) {
    const ret = getObject(arg0).location;
    return addHeapObject(ret);
};
imports.wbg.__wbg_history_370f36be0803466b = function() { return handleError(function (arg0) {
    const ret = getObject(arg0).history;
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_scrollTo_fa65cff02c3e7c5a = function(arg0, arg1, arg2) {
    getObject(arg0).scrollTo(arg1, arg2);
};
imports.wbg.__wbg_requestAnimationFrame_1820a8e6b645ec5a = function() { return handleError(function (arg0, arg1) {
    const ret = getObject(arg0).requestAnimationFrame(getObject(arg1));
    return ret;
}, arguments) };
imports.wbg.__wbg_fetch_ba9c8f5d941ae5c4 = function(arg0, arg1) {
    const ret = getObject(arg0).fetch(getObject(arg1));
    return addHeapObject(ret);
};
imports.wbg.__wbg_body_3eb73da919b867a1 = function(arg0) {
    const ret = getObject(arg0).body;
    return isLikeNone(ret) ? 0 : addHeapObject(ret);
};
imports.wbg.__wbg_createComment_ce9f467394242d45 = function(arg0, arg1, arg2) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    const ret = getObject(arg0).createComment(v0);
    return addHeapObject(ret);
};
imports.wbg.__wbg_createDocumentFragment_229f723f44e69ab9 = function(arg0) {
    const ret = getObject(arg0).createDocumentFragment();
    return addHeapObject(ret);
};
imports.wbg.__wbg_createElement_1a136faad4101f43 = function() { return handleError(function (arg0, arg1, arg2) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    const ret = getObject(arg0).createElement(v0);
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_createTextNode_dbdd908f92bae1b1 = function(arg0, arg1, arg2) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    const ret = getObject(arg0).createTextNode(v0);
    return addHeapObject(ret);
};
imports.wbg.__wbg_getElementById_00904c7c4a32c23b = function(arg0, arg1, arg2) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    const ret = getObject(arg0).getElementById(v0);
    return isLikeNone(ret) ? 0 : addHeapObject(ret);
};
imports.wbg.__wbg_setinnerHTML_99deeacfff0ae4cc = function(arg0, arg1, arg2) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    getObject(arg0).innerHTML = v0;
};
imports.wbg.__wbg_getAttribute_14ccdc738c4f7d95 = function(arg0, arg1, arg2, arg3) {
    var v0 = getCachedStringFromWasm0(arg2, arg3);
    const ret = getObject(arg1).getAttribute(v0);
    var ptr2 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    var len2 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len2;
    getInt32Memory0()[arg0 / 4 + 0] = ptr2;
};
imports.wbg.__wbg_hasAttribute_40947267e195b26c = function(arg0, arg1, arg2) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    const ret = getObject(arg0).hasAttribute(v0);
    return ret;
};
imports.wbg.__wbg_removeAttribute_5c264e727b67dbdb = function() { return handleError(function (arg0, arg1, arg2) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    getObject(arg0).removeAttribute(v0);
}, arguments) };
imports.wbg.__wbg_scrollIntoView_31df5d1d2bc727ab = function(arg0) {
    getObject(arg0).scrollIntoView();
};
imports.wbg.__wbg_setAttribute_0918ea45d5a1c663 = function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    var v1 = getCachedStringFromWasm0(arg3, arg4);
    getObject(arg0).setAttribute(v0, v1);
}, arguments) };
imports.wbg.__wbg_before_bed7b7b6e53dd469 = function() { return handleError(function (arg0, arg1) {
    getObject(arg0).before(getObject(arg1));
}, arguments) };
imports.wbg.__wbg_remove_ed2f62f1a8be044b = function(arg0) {
    getObject(arg0).remove();
};
imports.wbg.__wbg_target_791826e938c3e308 = function(arg0) {
    const ret = getObject(arg0).target;
    return isLikeNone(ret) ? 0 : addHeapObject(ret);
};
imports.wbg.__wbg_defaultPrevented_37035afe196e2aa0 = function(arg0) {
    const ret = getObject(arg0).defaultPrevented;
    return ret;
};
imports.wbg.__wbg_cancelBubble_191799b8e0ab3254 = function(arg0) {
    const ret = getObject(arg0).cancelBubble;
    return ret;
};
imports.wbg.__wbg_composedPath_d94a39b8c8f6eed1 = function(arg0) {
    const ret = getObject(arg0).composedPath();
    return addHeapObject(ret);
};
imports.wbg.__wbg_preventDefault_d2c7416966cb0632 = function(arg0) {
    getObject(arg0).preventDefault();
};
imports.wbg.__wbg_ctrlKey_0d75e0e9028bd999 = function(arg0) {
    const ret = getObject(arg0).ctrlKey;
    return ret;
};
imports.wbg.__wbg_shiftKey_12353f0e19b21d6a = function(arg0) {
    const ret = getObject(arg0).shiftKey;
    return ret;
};
imports.wbg.__wbg_altKey_a076f8612103d7e8 = function(arg0) {
    const ret = getObject(arg0).altKey;
    return ret;
};
imports.wbg.__wbg_metaKey_4e3f6e986f2802b1 = function(arg0) {
    const ret = getObject(arg0).metaKey;
    return ret;
};
imports.wbg.__wbg_button_8a97c55db17c7314 = function(arg0) {
    const ret = getObject(arg0).button;
    return ret;
};
imports.wbg.__wbg_new_26bb7e688dfc365c = function() { return handleError(function () {
    const ret = new URLSearchParams();
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_append_a0c110dbe340a46c = function(arg0, arg1, arg2, arg3, arg4) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    var v1 = getCachedStringFromWasm0(arg3, arg4);
    getObject(arg0).append(v0, v1);
};
imports.wbg.__wbg_length_d5ed87010607a669 = function(arg0) {
    const ret = getObject(arg0).length;
    return ret;
};
imports.wbg.__wbg_instanceof_ShadowRoot_cb6366cb0956ce29 = function(arg0) {
    let result;
    try {
        result = getObject(arg0) instanceof ShadowRoot;
    } catch (_) {
        result = false;
    }
    const ret = result;
    return ret;
};
imports.wbg.__wbg_host_99e27ed8897850f2 = function(arg0) {
    const ret = getObject(arg0).host;
    return addHeapObject(ret);
};
imports.wbg.__wbg_origin_028cdb6d9987f6c3 = function(arg0, arg1) {
    const ret = getObject(arg1).origin;
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len1;
    getInt32Memory0()[arg0 / 4 + 0] = ptr1;
};
imports.wbg.__wbg_pathname_d0d5b2fd2c7d8243 = function(arg0, arg1) {
    const ret = getObject(arg1).pathname;
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len1;
    getInt32Memory0()[arg0 / 4 + 0] = ptr1;
};
imports.wbg.__wbg_search_b5c7b044aaf64616 = function(arg0, arg1) {
    const ret = getObject(arg1).search;
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len1;
    getInt32Memory0()[arg0 / 4 + 0] = ptr1;
};
imports.wbg.__wbg_setsearch_ad0620e22e67a913 = function(arg0, arg1, arg2) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    getObject(arg0).search = v0;
};
imports.wbg.__wbg_searchParams_40c1f8d0a85de0aa = function(arg0) {
    const ret = getObject(arg0).searchParams;
    return addHeapObject(ret);
};
imports.wbg.__wbg_hash_286eced2921b7b34 = function(arg0, arg1) {
    const ret = getObject(arg1).hash;
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len1;
    getInt32Memory0()[arg0 / 4 + 0] = ptr1;
};
imports.wbg.__wbg_new_7d30e9d9d2deaf9d = function() { return handleError(function (arg0, arg1) {
    var v0 = getCachedStringFromWasm0(arg0, arg1);
    const ret = new URL(v0);
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_newwithbase_1151109a3f062f92 = function() { return handleError(function (arg0, arg1, arg2, arg3) {
    var v0 = getCachedStringFromWasm0(arg0, arg1);
    var v1 = getCachedStringFromWasm0(arg2, arg3);
    const ret = new URL(v0, v1);
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_instanceof_HtmlAnchorElement_afbec6867d883d45 = function(arg0) {
    let result;
    try {
        result = getObject(arg0) instanceof HTMLAnchorElement;
    } catch (_) {
        result = false;
    }
    const ret = result;
    return ret;
};
imports.wbg.__wbg_target_a012ddff98c775bb = function(arg0, arg1) {
    const ret = getObject(arg1).target;
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len1;
    getInt32Memory0()[arg0 / 4 + 0] = ptr1;
};
imports.wbg.__wbg_href_596cc35e5517d06b = function(arg0, arg1) {
    const ret = getObject(arg1).href;
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len1;
    getInt32Memory0()[arg0 / 4 + 0] = ptr1;
};
imports.wbg.__wbg_error_1f4e3e298a7c97f6 = function(arg0) {
    console.error(getObject(arg0));
};
imports.wbg.__wbg_log_9dfb3879776dd797 = function(arg0) {
    console.log(getObject(arg0));
};
imports.wbg.__wbg_warn_0e0204547af47087 = function(arg0) {
    console.warn(getObject(arg0));
};
imports.wbg.__wbg_append_517583bac5b5bb16 = function() { return handleError(function (arg0, arg1) {
    getObject(arg0).append(getObject(arg1));
}, arguments) };
imports.wbg.__wbg_new_a979e9eedc5e81a3 = function() { return handleError(function () {
    const ret = new Headers();
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_set_a4bc966cface36df = function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    var v1 = getCachedStringFromWasm0(arg3, arg4);
    getObject(arg0).set(v0, v1);
}, arguments) };
imports.wbg.__wbg_parentNode_f3957fdd408a62f7 = function(arg0) {
    const ret = getObject(arg0).parentNode;
    return isLikeNone(ret) ? 0 : addHeapObject(ret);
};
imports.wbg.__wbg_childNodes_75d3da5f3a7bb985 = function(arg0) {
    const ret = getObject(arg0).childNodes;
    return addHeapObject(ret);
};
imports.wbg.__wbg_previousSibling_4cd9e84aeb4df529 = function(arg0) {
    const ret = getObject(arg0).previousSibling;
    return isLikeNone(ret) ? 0 : addHeapObject(ret);
};
imports.wbg.__wbg_nextSibling_13e9454ef5323f1a = function(arg0) {
    const ret = getObject(arg0).nextSibling;
    return isLikeNone(ret) ? 0 : addHeapObject(ret);
};
imports.wbg.__wbg_appendChild_bd383ec5356c0bdb = function() { return handleError(function (arg0, arg1) {
    const ret = getObject(arg0).appendChild(getObject(arg1));
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_cloneNode_80501c66ab115588 = function() { return handleError(function (arg0) {
    const ret = getObject(arg0).cloneNode();
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_state_cabf8868613a7bdb = function() { return handleError(function (arg0) {
    const ret = getObject(arg0).state;
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_pushState_90b0a1cf59505502 = function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
    var v0 = getCachedStringFromWasm0(arg2, arg3);
    var v1 = getCachedStringFromWasm0(arg4, arg5);
    getObject(arg0).pushState(getObject(arg1), v0, v1);
}, arguments) };
imports.wbg.__wbg_replaceState_594fcc05da10fe45 = function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
    var v0 = getCachedStringFromWasm0(arg2, arg3);
    var v1 = getCachedStringFromWasm0(arg4, arg5);
    getObject(arg0).replaceState(getObject(arg1), v0, v1);
}, arguments) };
imports.wbg.__wbg_setchecked_3b12f3d602a63e47 = function(arg0, arg1) {
    getObject(arg0).checked = arg1 !== 0;
};
imports.wbg.__wbg_value_c93cb4b4d352228e = function(arg0, arg1) {
    const ret = getObject(arg1).value;
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len1;
    getInt32Memory0()[arg0 / 4 + 0] = ptr1;
};
imports.wbg.__wbg_setdata_4d5b377238fff97c = function(arg0, arg1, arg2) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    getObject(arg0).data = v0;
};
imports.wbg.__wbg_addEventListener_2f891d22985fd3c8 = function() { return handleError(function (arg0, arg1, arg2, arg3) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    getObject(arg0).addEventListener(v0, getObject(arg3));
}, arguments) };
imports.wbg.__wbg_addEventListener_1b158e9e95e0ab00 = function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    getObject(arg0).addEventListener(v0, getObject(arg3), getObject(arg4));
}, arguments) };
imports.wbg.__wbg_removeEventListener_07715e6f464823fc = function() { return handleError(function (arg0, arg1, arg2, arg3) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    getObject(arg0).removeEventListener(v0, getObject(arg3));
}, arguments) };
imports.wbg.__wbg_instanceof_WorkerGlobalScope_a9d2cb51ce9a4579 = function(arg0) {
    let result;
    try {
        result = getObject(arg0) instanceof WorkerGlobalScope;
    } catch (_) {
        result = false;
    }
    const ret = result;
    return ret;
};
imports.wbg.__wbg_fetch_06d656a1b748ac0d = function(arg0, arg1) {
    const ret = getObject(arg0).fetch(getObject(arg1));
    return addHeapObject(ret);
};
imports.wbg.__wbg_origin_a66ff95a994d7e40 = function() { return handleError(function (arg0, arg1) {
    const ret = getObject(arg1).origin;
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len1;
    getInt32Memory0()[arg0 / 4 + 0] = ptr1;
}, arguments) };
imports.wbg.__wbg_pathname_2cd8b46817926b06 = function() { return handleError(function (arg0, arg1) {
    const ret = getObject(arg1).pathname;
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len1;
    getInt32Memory0()[arg0 / 4 + 0] = ptr1;
}, arguments) };
imports.wbg.__wbg_search_eb68df82d26f8761 = function() { return handleError(function (arg0, arg1) {
    const ret = getObject(arg1).search;
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len1;
    getInt32Memory0()[arg0 / 4 + 0] = ptr1;
}, arguments) };
imports.wbg.__wbg_hash_9bd16c0f666cdf27 = function() { return handleError(function (arg0, arg1) {
    const ret = getObject(arg1).hash;
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len1;
    getInt32Memory0()[arg0 / 4 + 0] = ptr1;
}, arguments) };
imports.wbg.__wbg_url_70f3179afe0eccd6 = function(arg0, arg1) {
    const ret = getObject(arg1).url;
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len1;
    getInt32Memory0()[arg0 / 4 + 0] = ptr1;
};
imports.wbg.__wbg_newwithstr_19bf69d1840d2816 = function() { return handleError(function (arg0, arg1) {
    var v0 = getCachedStringFromWasm0(arg0, arg1);
    const ret = new Request(v0);
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_newwithstrandinit_9fd2fc855c6327eb = function() { return handleError(function (arg0, arg1, arg2) {
    var v0 = getCachedStringFromWasm0(arg0, arg1);
    const ret = new Request(v0, getObject(arg2));
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_instanceof_Response_0d25bb8436a9cefe = function(arg0) {
    let result;
    try {
        result = getObject(arg0) instanceof Response;
    } catch (_) {
        result = false;
    }
    const ret = result;
    return ret;
};
imports.wbg.__wbg_status_351700a30c61ba61 = function(arg0) {
    const ret = getObject(arg0).status;
    return ret;
};
imports.wbg.__wbg_text_10c88c5e55f873c7 = function() { return handleError(function (arg0) {
    const ret = getObject(arg0).text();
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_queueMicrotask_118eeb525d584d9a = function(arg0) {
    queueMicrotask(getObject(arg0));
};
imports.wbg.__wbg_queueMicrotask_26a89c14c53809c0 = function(arg0) {
    const ret = getObject(arg0).queueMicrotask;
    return addHeapObject(ret);
};
imports.wbg.__wbindgen_is_function = function(arg0) {
    const ret = typeof(getObject(arg0)) === 'function';
    return ret;
};
imports.wbg.__wbg_get_c43534c00f382c8a = function(arg0, arg1) {
    const ret = getObject(arg0)[arg1 >>> 0];
    return addHeapObject(ret);
};
imports.wbg.__wbg_length_d99b680fd68bf71b = function(arg0) {
    const ret = getObject(arg0).length;
    return ret;
};
imports.wbg.__wbg_new_34c624469fb1d4fd = function() {
    const ret = new Array();
    return addHeapObject(ret);
};
imports.wbg.__wbg_newnoargs_5859b6d41c6fe9f7 = function(arg0, arg1) {
    var v0 = getCachedStringFromWasm0(arg0, arg1);
    const ret = new Function(v0);
    return addHeapObject(ret);
};
imports.wbg.__wbg_next_1938cf110c9491d4 = function(arg0) {
    const ret = getObject(arg0).next;
    return addHeapObject(ret);
};
imports.wbg.__wbg_next_267398d0e0761bf9 = function() { return handleError(function (arg0) {
    const ret = getObject(arg0).next();
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_done_506b44765ba84b9c = function(arg0) {
    const ret = getObject(arg0).done;
    return ret;
};
imports.wbg.__wbg_value_31485d8770eb06ab = function(arg0) {
    const ret = getObject(arg0).value;
    return addHeapObject(ret);
};
imports.wbg.__wbg_iterator_364187e1ee96b750 = function() {
    const ret = Symbol.iterator;
    return addHeapObject(ret);
};
imports.wbg.__wbg_get_5027b32da70f39b1 = function() { return handleError(function (arg0, arg1) {
    const ret = Reflect.get(getObject(arg0), getObject(arg1));
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_call_a79f1973a4f07d5e = function() { return handleError(function (arg0, arg1) {
    const ret = getObject(arg0).call(getObject(arg1));
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_new_87d841e70661f6e9 = function() {
    const ret = new Object();
    return addHeapObject(ret);
};
imports.wbg.__wbg_self_086b5302bcafb962 = function() { return handleError(function () {
    const ret = self.self;
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_window_132fa5d7546f1de5 = function() { return handleError(function () {
    const ret = window.window;
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_globalThis_e5f801a37ad7d07b = function() { return handleError(function () {
    const ret = globalThis.globalThis;
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_global_f9a61fce4af6b7c1 = function() { return handleError(function () {
    const ret = global.global;
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_decodeURI_4bf318e70843e94f = function() { return handleError(function (arg0, arg1) {
    var v0 = getCachedStringFromWasm0(arg0, arg1);
    const ret = decodeURI(v0);
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_encodeURIComponent_8a6e310fdf61d07d = function(arg0, arg1) {
    var v0 = getCachedStringFromWasm0(arg0, arg1);
    const ret = encodeURIComponent(v0);
    return addHeapObject(ret);
};
imports.wbg.__wbg_set_379b27f1d5f1bf9c = function(arg0, arg1, arg2) {
    getObject(arg0)[arg1 >>> 0] = takeObject(arg2);
};
imports.wbg.__wbg_isArray_fbd24d447869b527 = function(arg0) {
    const ret = Array.isArray(getObject(arg0));
    return ret;
};
imports.wbg.__wbg_instanceof_ArrayBuffer_f4521cec1b99ee35 = function(arg0) {
    let result;
    try {
        result = getObject(arg0) instanceof ArrayBuffer;
    } catch (_) {
        result = false;
    }
    const ret = result;
    return ret;
};
imports.wbg.__wbg_instanceof_Error_f5ae6a28929a8190 = function(arg0) {
    let result;
    try {
        result = getObject(arg0) instanceof Error;
    } catch (_) {
        result = false;
    }
    const ret = result;
    return ret;
};
imports.wbg.__wbg_message_5dbdf59ed61bbc49 = function(arg0) {
    const ret = getObject(arg0).message;
    return addHeapObject(ret);
};
imports.wbg.__wbg_name_90a0336d27b12317 = function(arg0) {
    const ret = getObject(arg0).name;
    return addHeapObject(ret);
};
imports.wbg.__wbg_toString_5326377607a05bf2 = function(arg0) {
    const ret = getObject(arg0).toString();
    return addHeapObject(ret);
};
imports.wbg.__wbg_call_f6a2bc58c19c53c6 = function() { return handleError(function (arg0, arg1, arg2) {
    const ret = getObject(arg0).call(getObject(arg1), getObject(arg2));
    return addHeapObject(ret);
}, arguments) };
imports.wbg.__wbg_isSafeInteger_d8c89788832a17bf = function(arg0) {
    const ret = Number.isSafeInteger(getObject(arg0));
    return ret;
};
imports.wbg.__wbg_getTime_af7ca51c0bcefa08 = function(arg0) {
    const ret = getObject(arg0).getTime();
    return ret;
};
imports.wbg.__wbg_getTimezoneOffset_98604170efd7a383 = function(arg0) {
    const ret = getObject(arg0).getTimezoneOffset();
    return ret;
};
imports.wbg.__wbg_new0_c0e40662db0749ee = function() {
    const ret = new Date();
    return addHeapObject(ret);
};
imports.wbg.__wbg_now_86f7ca537c8b86d5 = function() {
    const ret = Date.now();
    return ret;
};
imports.wbg.__wbg_is_a5728dbfb61c82cd = function(arg0, arg1) {
    const ret = Object.is(getObject(arg0), getObject(arg1));
    return ret;
};
imports.wbg.__wbg_toString_61d1ba76c783d2bc = function(arg0) {
    const ret = getObject(arg0).toString();
    return addHeapObject(ret);
};
imports.wbg.__wbg_exec_70dc2e84f7a29584 = function(arg0, arg1, arg2) {
    var v0 = getCachedStringFromWasm0(arg1, arg2);
    const ret = getObject(arg0).exec(v0);
    return isLikeNone(ret) ? 0 : addHeapObject(ret);
};
imports.wbg.__wbg_new_c62202c5c4bd9009 = function(arg0, arg1, arg2, arg3) {
    var v0 = getCachedStringFromWasm0(arg0, arg1);
    var v1 = getCachedStringFromWasm0(arg2, arg3);
    const ret = new RegExp(v0, v1);
    return addHeapObject(ret);
};
imports.wbg.__wbg_resolve_97ecd55ee839391b = function(arg0) {
    const ret = Promise.resolve(getObject(arg0));
    return addHeapObject(ret);
};
imports.wbg.__wbg_then_7aeb7c5f1536640f = function(arg0, arg1) {
    const ret = getObject(arg0).then(getObject(arg1));
    return addHeapObject(ret);
};
imports.wbg.__wbg_then_5842e4e97f7beace = function(arg0, arg1, arg2) {
    const ret = getObject(arg0).then(getObject(arg1), getObject(arg2));
    return addHeapObject(ret);
};
imports.wbg.__wbg_buffer_5d1b598a01b41a42 = function(arg0) {
    const ret = getObject(arg0).buffer;
    return addHeapObject(ret);
};
imports.wbg.__wbg_new_ace717933ad7117f = function(arg0) {
    const ret = new Uint8Array(getObject(arg0));
    return addHeapObject(ret);
};
imports.wbg.__wbg_set_74906aa30864df5a = function(arg0, arg1, arg2) {
    getObject(arg0).set(getObject(arg1), arg2 >>> 0);
};
imports.wbg.__wbg_length_f0764416ba5bb237 = function(arg0) {
    const ret = getObject(arg0).length;
    return ret;
};
imports.wbg.__wbg_instanceof_Uint8Array_4f5cffed7df34b2f = function(arg0) {
    let result;
    try {
        result = getObject(arg0) instanceof Uint8Array;
    } catch (_) {
        result = false;
    }
    const ret = result;
    return ret;
};
imports.wbg.__wbg_set_37a50e901587b477 = function() { return handleError(function (arg0, arg1, arg2) {
    const ret = Reflect.set(getObject(arg0), getObject(arg1), getObject(arg2));
    return ret;
}, arguments) };
imports.wbg.__wbindgen_bigint_get_as_i64 = function(arg0, arg1) {
    const v = getObject(arg1);
    const ret = typeof(v) === 'bigint' ? v : undefined;
    getBigInt64Memory0()[arg0 / 8 + 1] = isLikeNone(ret) ? BigInt(0) : ret;
    getInt32Memory0()[arg0 / 4 + 0] = !isLikeNone(ret);
};
imports.wbg.__wbindgen_debug_string = function(arg0, arg1) {
    const ret = debugString(getObject(arg1));
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getInt32Memory0()[arg0 / 4 + 1] = len1;
    getInt32Memory0()[arg0 / 4 + 0] = ptr1;
};
imports.wbg.__wbindgen_throw = function(arg0, arg1) {
    throw new Error(getStringFromWasm0(arg0, arg1));
};
imports.wbg.__wbindgen_memory = function() {
    const ret = wasm.memory;
    return addHeapObject(ret);
};
imports.wbg.__wbindgen_closure_wrapper928 = function(arg0, arg1, arg2) {
    const ret = makeClosure(arg0, arg1, 328, __wbg_adapter_54);
    return addHeapObject(ret);
};
imports.wbg.__wbindgen_closure_wrapper929 = function(arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 328, __wbg_adapter_57);
    return addHeapObject(ret);
};
imports.wbg.__wbindgen_closure_wrapper932 = function(arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 328, __wbg_adapter_57);
    return addHeapObject(ret);
};
imports.wbg.__wbindgen_closure_wrapper934 = function(arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 328, __wbg_adapter_62);
    return addHeapObject(ret);
};
imports.wbg.__wbindgen_closure_wrapper1755 = function(arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 742, __wbg_adapter_65);
    return addHeapObject(ret);
};
imports.wbg.__wbindgen_closure_wrapper2404 = function(arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 927, __wbg_adapter_68);
    return addHeapObject(ret);
};
imports.wbg.__wbindgen_closure_wrapper2585 = function(arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 973, __wbg_adapter_71);
    return addHeapObject(ret);
};
imports.wbg.__wbindgen_closure_wrapper3923 = function(arg0, arg1, arg2) {
    const ret = makeMutClosure(arg0, arg1, 991, __wbg_adapter_74);
    return addHeapObject(ret);
};

return imports;
}

function __wbg_init_memory(imports, maybe_memory) {

}

function __wbg_finalize_init(instance, module) {
    wasm = instance.exports;
    __wbg_init.__wbindgen_wasm_module = module;
    cachedBigInt64Memory0 = null;
    cachedFloat64Memory0 = null;
    cachedInt32Memory0 = null;
    cachedUint32Memory0 = null;
    cachedUint8Memory0 = null;

    wasm.__wbindgen_start();
    return wasm;
}

function initSync(module) {
    if (wasm !== undefined) return wasm;

    const imports = __wbg_get_imports();

    __wbg_init_memory(imports);

    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }

    const instance = new WebAssembly.Instance(module, imports);

    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(input) {
    if (wasm !== undefined) return wasm;

    if (typeof input === 'undefined') {
        input = new URL('tracer-ui-c74352e5caaa0f10_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof input === 'string' || (typeof Request === 'function' && input instanceof Request) || (typeof URL === 'function' && input instanceof URL)) {
        input = fetch(input);
    }

    __wbg_init_memory(imports);

    const { instance, module } = await __wbg_load(await input, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync }
export default __wbg_init;
