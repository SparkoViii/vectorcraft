//! What scripts see: the global `vc` and `alert`, backed by the session the run moved in.

use std::cell::RefCell;
use std::rc::Rc;

use boa_engine::error::PanicError;
use boa_engine::object::ObjectInitializer;
use boa_engine::property::Attribute;
use boa_engine::{Context, JsNativeError, JsObject, JsResult, JsValue, NativeFunction, js_string};
use serde_json::{Value, json};
use vectorcraft_engine::{BatchRun, Session};

use crate::{Limits, convert};

/// Log lines kept per run, and their total size.
const MAX_LOG_LINES: usize = 10_000;
const MAX_LOG_BYTES: usize = 1 << 20;
/// Alerts kept per run.
const MAX_ALERTS: usize = 100;

/// The run's state, shared with the native functions through the context's data.
pub(crate) struct Host {
    pub(crate) session: Session,
    pub(crate) batch: BatchRun,
    pub(crate) limits: Limits,
    pub(crate) log: Vec<String>,
    log_bytes: usize,
    log_truncated: bool,
    pub(crate) alerts: Vec<String>,
    pub(crate) commands: usize,
    /// Why the host stopped the script (a limit), which no `catch` sees.
    pub(crate) stopped: Option<String>,
    /// Promises rejected with nothing handling them yet ([`crate::hooks`]).
    pub(crate) unhandled: Vec<JsObject>,
}

pub(crate) type Shared = Rc<RefCell<Option<Host>>>;

impl Host {
    pub(crate) fn new(session: Session, batch: BatchRun, limits: Limits) -> Self {
        Self {
            session,
            batch,
            limits,
            log: vec![],
            log_bytes: 0,
            log_truncated: false,
            alerts: vec![],
            commands: 0,
            stopped: None,
            unhandled: vec![],
        }
    }

    fn push_log(&mut self, line: String) {
        if self.log.len() >= MAX_LOG_LINES || self.log_bytes + line.len() > MAX_LOG_BYTES {
            if !self.log_truncated {
                self.log.push("(log truncated)".into());
                self.log_truncated = true;
            }
            return;
        }
        self.log_bytes += line.len();
        self.log.push(line);
    }
}

fn with_host<T>(ctx: &Context, f: impl FnOnce(&mut Host) -> JsResult<T>) -> JsResult<T> {
    let shared = ctx.get_data::<Shared>().cloned().ok_or_else(|| JsNativeError::error().with_message("the script host is gone"))?;
    let mut slot = shared.try_borrow_mut().map_err(|_| JsNativeError::error().with_message("the script host is busy"))?;
    let host = slot.as_mut().ok_or_else(|| JsNativeError::error().with_message("the script host is gone"))?;
    f(host)
}

/// A value as `vc.log` and `alert` show it: strings as they are, the rest as JSON or text.
fn text_of(v: &JsValue, ctx: &mut Context) -> String {
    if let Some(s) = v.as_string() {
        return s.to_std_string_escaped();
    }
    match convert::to_json(v, ctx) {
        Ok(j) => j.to_string(),
        Err(e) => convert::describe(v, &e, ctx),
    }
}

/// `vc.run(id, params?)` → the command's result; a failing command throws an `Error`.
fn vc_run(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let id = args
        .first()
        .and_then(JsValue::as_string)
        .map(|s| s.to_std_string_escaped())
        .ok_or_else(|| JsNativeError::typ().with_message("vc.run: the first argument is a command id, such as \"shape.rectangle\""))?;
    let params = match args.get(1) {
        None => json!({}),
        Some(v) if v.is_undefined() || v.is_null() => json!({}),
        Some(v) => {
            let p = convert::to_json(v, ctx)?;
            if !p.is_object() {
                return Err(JsNativeError::typ().with_message("vc.run: params must be an object").into());
            }
            p
        }
    };
    let result = with_host(ctx, |h| {
        if h.commands >= h.limits.commands {
            let msg = format!("the script ran more than {} commands", h.limits.commands);
            h.stopped = Some(msg.clone());
            // An engine error: no `catch` in the script sees it.
            return Err(PanicError::new(msg).into());
        }
        h.commands += 1;
        let Host { session, batch, .. } = h;
        Ok(session.batch_step(batch, &id, &params).map(|(v, _)| v))
    })?;
    match result {
        Ok(v) => convert::from_json(&v, ctx),
        Err(e) => Err(JsNativeError::error().with_message(e.to_string()).into()),
    }
}

/// `vc.document()` → the active document as `document.inspect` returns it, or null.
fn vc_document(_: &JsValue, _: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let v = with_host(ctx, |h| Ok(h.session.active().map(|_| vectorcraft_engine::inspect::document(&h.session))))?;
    match v {
        Some(v) => convert::from_json(&v, ctx),
        None => Ok(JsValue::null()),
    }
}

/// `vc.commands()` → `[{id, label, params}]`.
fn vc_commands(_: &JsValue, _: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let list: Vec<Value> = vectorcraft_engine::command_specs().iter().map(|c| json!({"id": c.id, "label": c.label, "params": c.params})).collect();
    convert::from_json(&Value::Array(list), ctx)
}

/// `vc.log(...values)`: one line in the run's log.
fn vc_log(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let line = args.iter().map(|a| text_of(a, ctx)).collect::<Vec<_>>().join(" ");
    with_host(ctx, |h| {
        h.push_log(line);
        Ok(())
    })?;
    Ok(JsValue::undefined())
}

/// `alert(message)`: shown after the run (it doesn't wait).
fn alert(_: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let msg = args.first().map(|a| text_of(a, ctx)).unwrap_or_default();
    with_host(ctx, |h| {
        if h.alerts.len() < MAX_ALERTS {
            h.alerts.push(msg);
        }
        Ok(())
    })?;
    Ok(JsValue::undefined())
}

/// Define `vc` and `alert`, the only host bindings (no files, network or OS).
pub(crate) fn install(ctx: &mut Context, args: &serde_json::Map<String, Value>) -> JsResult<()> {
    let args = convert::from_json(&Value::Object(args.clone()), ctx)?;
    let vc = ObjectInitializer::new(ctx)
        .function(NativeFunction::from_fn_ptr(vc_run), js_string!("run"), 2)
        .function(NativeFunction::from_fn_ptr(vc_document), js_string!("document"), 0)
        .function(NativeFunction::from_fn_ptr(vc_commands), js_string!("commands"), 0)
        .function(NativeFunction::from_fn_ptr(vc_log), js_string!("log"), 0)
        .property(js_string!("args"), args, Attribute::READONLY | Attribute::ENUMERABLE)
        .build();
    ctx.register_global_property(js_string!("vc"), vc, Attribute::READONLY | Attribute::NON_ENUMERABLE)?;
    ctx.register_global_callable(js_string!("alert"), 1, NativeFunction::from_fn_ptr(alert))?;
    Ok(())
}
