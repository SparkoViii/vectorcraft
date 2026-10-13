//! Script errors, with the position Boa reports.

use boa_engine::error::TryNativeError;
use boa_engine::{Context, JsError};

/// Why a script failed, and where.
#[derive(Clone, Debug, PartialEq)]
pub struct ScriptError {
    pub message: String,
    /// The script's file name.
    pub file: String,
    /// 1-based, when the engine reported a position.
    pub line: Option<u32>,
    pub column: Option<u32>,
    /// What the script logged before it failed (`vc.log`).
    pub log: Vec<String>,
}

impl ScriptError {
    pub(crate) fn new(file: &str, message: impl Into<String>) -> Self {
        Self { message: message.into(), file: file.to_string(), line: None, column: None, log: vec![] }
    }

    pub(crate) fn from_js(e: &JsError, ctx: &mut Context, file: &str) -> Self {
        // An `Error` object knows where it was made; anything else thrown (a string, a limit)
        // carries the place in its backtrace lines.
        let (text, backtrace) = match e.try_native(ctx) {
            Ok(native) => (native.to_string(), true),
            // A limit stops the engine wherever it is: its backtrace names the calls before the
            // loop, not the loop, so it gives no place.
            Err(TryNativeError::EngineError { .. }) => (e.to_string(), false),
            Err(_) => (e.to_string(), true),
        };
        let (message, at) = split_position(&text, backtrace);
        Self { message, file: file.to_string(), line: at.map(|p| p.0), column: at.map(|p| p.1), log: vec![] }
    }
}

impl std::fmt::Display for ScriptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.line, self.column) {
            (Some(l), Some(c)) => write!(f, "{}:{l}:{c}: {}", self.file, self.message),
            _ => write!(f, "{}: {}", self.file, self.message),
        }
    }
}

impl std::error::Error for ScriptError {}

/// Boa's error text → its first line without the position, and (line, column). Syntax errors end
/// `… at line 3, col 7 (native)`; runtime errors `… (name.js:3:7)` (`(name.js:?:?)` when unknown),
/// followed by backtrace lines `    at f (name.js:3:7)`, which give the position of what isn't an
/// `Error` object (a thrown string), read when `backtrace` is set.
pub(crate) fn split_position(text: &str, backtrace: bool) -> (String, Option<(u32, u32)>) {
    let mut lines = text.lines();
    let first = lines.next().unwrap_or_default();
    let head = first.strip_suffix(" (native)").unwrap_or(first);
    if let Some(i) = head.rfind(" at line ")
        && let Some((l, c)) = head.get(i + 9..).and_then(|t| t.split_once(", col "))
        && let (Ok(l), Ok(c)) = (l.trim().parse(), c.trim().parse())
    {
        return (head.get(..i).unwrap_or(head).to_string(), Some((l, c)));
    }
    let (message, mut at) = match trailing_position(head) {
        Some((rest, at)) => (rest.to_string(), at),
        None => (head.to_string(), None),
    };
    if at.is_none() && backtrace {
        at = lines.find_map(|l| trailing_position(l.trim()).and_then(|(_, at)| at));
    }
    (message, at)
}

/// `text (file:L:C)` → (`text`, Some((L, C))); `text (file:?:?)` → (`text`, None); else None.
fn trailing_position(text: &str) -> Option<(&str, Option<(u32, u32)>)> {
    if !text.ends_with(')') {
        return None;
    }
    let open = text.rfind(" (")?;
    let inner = text.get(open + 2..text.len() - 1)?;
    let mut parts = inner.rsplitn(3, ':');
    let (c, l, _file) = (parts.next()?, parts.next()?, parts.next()?);
    let rest = text.get(..open)?;
    if (l, c) == ("?", "?") {
        return Some((rest, None));
    }
    match (l.parse(), c.parse()) {
        (Ok(l), Ok(c)) => Some((rest, Some((l, c)))),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_are_split_off() {
        assert_eq!(
            split_position("SyntaxError: unexpected token ';', primary expression at line 1, col 9 (native)", true),
            ("SyntaxError: unexpected token ';', primary expression".into(), Some((1, 9)))
        );
        assert_eq!(split_position("Error: kaput (test.js:3:7)", true), ("Error: kaput".into(), Some((3, 7))));
        assert_eq!(split_position("ReferenceError: x is not defined (test.js:?:?)", true), ("ReferenceError: x is not defined".into(), None));
        assert_eq!(split_position("\"plain\"\n    at <main> (test.js:1:7)", true), ("\"plain\"".into(), Some((1, 7))));
        assert_eq!(
            split_position("RuntimeLimitError: reached the maximum number of iteration loops on this execution\n    at <main> (t.js:?:?)", true),
            ("RuntimeLimitError: reached the maximum number of iteration loops on this execution".into(), None)
        );
        assert_eq!(split_position("Error: (nothing) here", true), ("Error: (nothing) here".into(), None));
        assert_eq!(split_position("plain", true), ("plain".into(), None));
        assert_eq!(split_position("", true), (String::new(), None));
        assert_eq!(split_position("RuntimeLimitError: loops\n    at <main> (t.js:1:7)", false), ("RuntimeLimitError: loops".into(), None));
    }
}
