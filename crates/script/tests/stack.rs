//! Scripts run on a thread with a large stack, so built-ins that recurse natively (stringifying
//! deeply nested arrays) don't overflow the caller's (a test thread's 2 MiB, Windows' 1 MiB main
//! thread). On its own test binary: an overflow aborts the whole process.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;
use vectorcraft_engine::Session;
use vectorcraft_script::{ScriptOptions, run_script};

#[test]
fn deep_values_built_at_run_time_use_the_scripts_own_stack() {
    let mut s = Session::new();
    let src = "let a = []; for (let i = 0; i < 20000; i++) a = [a]; String(a).length + JSON.stringify(a).length";
    let out = run_script(&mut s, src, "deep.js", &ScriptOptions::default()).unwrap();
    // String(a) is "" (each level joins one empty item); JSON.stringify(a) is 20,001 "[" and "]".
    assert_eq!(out.value, json!(40002));
}
