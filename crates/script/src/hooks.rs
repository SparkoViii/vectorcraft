//! Boa's host hooks: strings compiled while a script runs (`eval`, `new Function`) get the same
//! nesting check as the script ([`crate::scan`]), and a promise rejected with nothing to handle it
//! fails the run, as an uncaught throw does.

use boa_engine::builtins::promise::OperationType;
use boa_engine::builtins::promise::Promise;
use boa_engine::context::HostHooks;
use boa_engine::object::FunctionObjectBuilder;
use boa_engine::realm::Realm;
use boa_engine::{Context, JsNativeError, JsObject, JsResult, JsString, JsValue, NativeFunction, js_string};

use crate::host::Shared;
use crate::scan;

pub(crate) struct Hooks;

impl HostHooks for Hooks {
    fn ensure_can_compile_strings(
        &self,
        _realm: Realm,
        parameters: &[JsString],
        body: &JsString,
        _direct: bool,
        _context: &mut Context,
    ) -> JsResult<()> {
        for text in parameters.iter().chain(std::iter::once(body)) {
            if let Err(e) = scan::check(&text.to_std_string_escaped()) {
                return Err(JsNativeError::range().with_message(e.message).into());
            }
        }
        Ok(())
    }

    fn promise_rejection_tracker(&self, promise: &JsObject<Promise>, operation: OperationType, context: &mut Context) {
        let Some(shared) = context.get_data::<Shared>().cloned() else { return };
        let Ok(mut slot) = shared.try_borrow_mut() else { return };
        let Some(host) = slot.as_mut() else { return };
        let promise = promise.clone().upcast();
        match operation {
            OperationType::Reject => host.unhandled.push(promise),
            OperationType::Handle => host.unhandled.retain(|p| p != &promise),
        }
    }
}

/// `JSON.parse` refuses text nested deeper than the parser's stack allows ([`scan::check_json`]).
pub(crate) fn guard_json_parse(ctx: &mut Context) -> JsResult<()> {
    let json = ctx.global_object().get(js_string!("JSON"), ctx)?;
    let Some(json) = json.as_object() else { return Ok(()) };
    let Some(parse) = json.get(js_string!("parse"), ctx)?.as_object() else { return Ok(()) };
    let guarded = NativeFunction::from_copy_closure_with_captures(
        |this: &JsValue, args: &[JsValue], original: &JsObject, ctx: &mut Context| -> JsResult<JsValue> {
            if let Some(text) = args.first().and_then(JsValue::as_string) {
                scan::check_json(&text.to_std_string_escaped()).map_err(|m| JsNativeError::syntax().with_message(m))?;
            }
            original.call(this, args, ctx)
        },
        parse,
    );
    let realm = ctx.realm().clone();
    let f = FunctionObjectBuilder::new(&realm, guarded).name(js_string!("parse")).length(2).build();
    json.set(js_string!("parse"), f, false, ctx)?;
    Ok(())
}
