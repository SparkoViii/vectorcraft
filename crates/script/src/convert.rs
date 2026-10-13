//! Values between scripts and commands: command params and results are JSON.

use boa_engine::{Context, JsError, JsNativeError, JsObject, JsResult, JsValue, js_string};
use serde_json::Value;

/// Deepest nesting passed to a command.
pub(crate) const MAX_DEPTH: usize = 256;
/// Most elements (or keys) in one array (or object) passed to a command.
pub(crate) const MAX_LENGTH: u64 = 1_000_000;
/// Most values in all passed to a command: a value that reuses the same objects (`o = [o, o]`)
/// would otherwise be walked an exponential number of times.
pub(crate) const MAX_VALUES: usize = 1_000_000;

fn type_error(msg: impl Into<String>) -> JsError {
    JsNativeError::typ().with_message(msg.into()).into()
}

/// A command's JSON as a script value.
pub(crate) fn from_json(v: &Value, ctx: &mut Context) -> JsResult<JsValue> {
    JsValue::from_json(v, ctx)
}

/// A script value as JSON, as `JSON.stringify` would see it, refusing what isn't JSON (functions,
/// symbols, BigInts, cycles, NaN and Infinity) with a `TypeError` instead of dropping it.
pub(crate) fn to_json(v: &JsValue, ctx: &mut Context) -> JsResult<Value> {
    let mut walk = Walk { seen: Vec::new(), left: MAX_VALUES };
    walk.value(v, ctx, 0)
}

/// A value that isn't JSON as `vc.log`, `alert` and a script's value show it: a function's
/// source, a primitive's text, else why it isn't JSON (turning a big object into text could
/// itself run for ever).
pub(crate) fn describe(v: &JsValue, why: &JsError, ctx: &mut Context) -> String {
    if (v.as_callable().is_some() || !v.is_object())
        && let Ok(s) = v.to_string(ctx)
    {
        return s.to_std_string_escaped();
    }
    format!("[not JSON: {why}]")
}

struct Walk {
    /// The objects being walked (their ancestors): a cycle.
    seen: Vec<JsObject>,
    /// Values still allowed ([`MAX_VALUES`]).
    left: usize,
}

impl Walk {
    fn value(&mut self, v: &JsValue, ctx: &mut Context, depth: usize) -> JsResult<Value> {
        if self.left == 0 {
            return Err(type_error(format!("values with more than {MAX_VALUES} entries in all are not passed to commands")));
        }
        self.left -= 1;
        if depth > MAX_DEPTH {
            return Err(type_error(format!("values nested more than {MAX_DEPTH} levels deep are not passed to commands")));
        }
        if v.is_null() || v.is_undefined() {
            return Ok(Value::Null);
        }
        if let Some(b) = v.as_boolean() {
            return Ok(Value::Bool(b));
        }
        if let Some(s) = v.as_string() {
            return Ok(Value::String(s.to_std_string_escaped()));
        }
        if let Some(n) = v.as_number() {
            return number(n);
        }
        if v.is_bigint() {
            return Err(type_error("a BigInt is not JSON: convert it with Number() or String()"));
        }
        if v.is_symbol() {
            return Err(type_error("a Symbol is not JSON"));
        }
        let Some(obj) = v.as_object() else { return Err(type_error("not a JSON value")) };
        if obj.is_callable() {
            return Err(type_error("a function is not JSON"));
        }
        if self.seen.contains(&obj) {
            return Err(type_error("a cyclic object is not JSON"));
        }
        self.seen.push(obj.clone());
        let r = if obj.is_array() { self.array(&obj, ctx, depth) } else { self.object(&obj, ctx, depth) };
        self.seen.pop();
        r
    }
}

fn number(n: f64) -> JsResult<Value> {
    if !n.is_finite() {
        return Err(type_error("NaN and Infinity are not JSON"));
    }
    // Whole numbers stay integers (ids, counts); the cast can't overflow below 2^53.
    if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_991.0 {
        return Ok(Value::from(n as i64));
    }
    serde_json::Number::from_f64(n).map(Value::Number).ok_or_else(|| type_error("not a JSON number"))
}

fn length(obj: &JsObject, ctx: &mut Context, what: &str) -> JsResult<u32> {
    let n = obj.get(js_string!("length"), ctx)?.to_length(ctx)?;
    if n > MAX_LENGTH {
        return Err(type_error(format!("{what} with more than {MAX_LENGTH} entries are not passed to commands")));
    }
    u32::try_from(n).map_err(|_| type_error("too long"))
}

impl Walk {
    fn array(&mut self, obj: &JsObject, ctx: &mut Context, depth: usize) -> JsResult<Value> {
        let n = length(obj, ctx, "arrays")?;
        let mut out = Vec::with_capacity(n as usize);
        for i in 0..n {
            let item = obj.get(i, ctx)?;
            out.push(self.value(&item, ctx, depth + 1)?);
        }
        Ok(Value::Array(out))
    }

    fn object(&mut self, obj: &JsObject, ctx: &mut Context, depth: usize) -> JsResult<Value> {
        // Its own enumerable string keys, in JSON.stringify's order.
        let keys_fn = ctx.intrinsics().constructors().object().constructor().get(js_string!("keys"), ctx)?;
        let keys_fn = keys_fn.as_callable().ok_or_else(|| type_error("Object.keys is not a function"))?;
        let keys = keys_fn.call(&JsValue::undefined(), &[obj.clone().into()], ctx)?;
        let keys = keys.as_object().ok_or_else(|| type_error("Object.keys returned no array"))?;
        let n = length(&keys, ctx, "objects")?;
        let mut map = serde_json::Map::new();
        for i in 0..n {
            let key = keys.get(i, ctx)?.to_string(ctx)?;
            let value = obj.get(key.clone(), ctx)?;
            if value.is_undefined() {
                continue;
            }
            map.insert(key.to_std_string_escaped(), self.value(&value, ctx, depth + 1)?);
        }
        Ok(Value::Object(map))
    }
}

#[cfg(test)]
mod tests {
    use boa_engine::{Context, Source};
    use serde_json::json;

    use super::*;

    fn context() -> Context {
        Context::builder().instructions_remaining(100_000_000).build().unwrap()
    }

    fn convert(src: &str) -> Result<Value, String> {
        let mut ctx = context();
        let v = ctx.eval(Source::from_bytes(src.as_bytes())).unwrap();
        to_json(&v, &mut ctx).map_err(|e| e.to_string())
    }

    #[test]
    fn plain_values_round_trip() {
        assert_eq!(convert("({a: 1, b: [true, null, 'x'], c: {d: -2.5}})"), Ok(json!({"a": 1, "b": [true, null, "x"], "c": {"d": -2.5}})));
        assert_eq!(convert("9007199254740991"), Ok(json!(9007199254740991_i64)));
        assert_eq!(convert("1e300"), Ok(json!(1e300)));
        assert_eq!(convert("-0"), Ok(json!(0)));
        assert_eq!(convert("'héllo \\u{1F600}'"), Ok(json!("héllo 😀")));
        let mut ctx = context();
        let back = from_json(&json!({"n": [1, 2.5, "s", null, {"k": false}]}), &mut ctx).unwrap();
        assert_eq!(to_json(&back, &mut ctx).unwrap(), json!({"n": [1, 2.5, "s", null, {"k": false}]}));
    }

    #[test]
    fn undefined_follows_json_stringify() {
        assert_eq!(convert("({a: undefined, b: 1})"), Ok(json!({"b": 1})));
        assert_eq!(convert("[undefined, 1]"), Ok(json!([null, 1])));
        assert_eq!(convert("undefined"), Ok(Value::Null));
    }

    #[test]
    fn non_json_values_are_type_errors() {
        for src in ["(() => 1)", "Symbol('s')", "10n", "NaN", "Infinity", "({f() {}}).f", "(() => { const o = {}; o.self = o; return o; })()"] {
            let e = convert(src).unwrap_err();
            assert!(e.contains("TypeError"), "{src}: {e}");
        }
    }

    #[test]
    fn huge_or_deep_values_are_refused_quickly() {
        assert!(convert("(() => { const a = []; a[1e9] = 1; return a; })()").unwrap_err().contains("TypeError"));
        assert!(convert("(() => { let o = {}; for (let i = 0; i < 10000; i++) o = {o}; return o; })()").unwrap_err().contains("TypeError"));
    }

    #[test]
    fn a_throwing_getter_is_an_error_not_a_panic() {
        assert!(convert("({ get x() { throw new Error('nope'); } })").unwrap_err().contains("nope"));
    }

    #[test]
    fn lone_surrogates_survive_as_escapes() {
        assert!(convert("'\\uD800'").unwrap().as_str().is_some());
    }
}
