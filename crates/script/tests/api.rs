//! The script API: vc.run / document / commands / log / args, alert, one undo step, roll back,
//! limits and error positions.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Map, Value, json};
use vectorcraft_engine::Session;
use vectorcraft_script::{Limits, ScriptOptions, read_source, run_script};

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 300})).unwrap();
    s
}

fn count(s: &Session) -> usize {
    s.doc().unwrap().doc.layers[0].children().unwrap().len()
}

fn labels(s: &Session) -> Vec<String> {
    s.doc().unwrap().history.undo.iter().map(|h| h.label.clone()).collect()
}

fn run(s: &mut Session, src: &str) -> Result<vectorcraft_script::ScriptOutcome, vectorcraft_script::ScriptError> {
    run_script(s, src, "test.js", &ScriptOptions::default())
}

const RECT: &str = "vc.run('shape.rectangle', {x: 10, y: 10, width: 50, height: 40})";

#[test]
fn a_run_draws_and_is_one_undo_step() {
    let mut s = session();
    let out = run(&mut s, &format!("{RECT}; {RECT}; vc.document().layers[0].children.length")).unwrap();
    assert_eq!(out.value, json!(2));
    assert_eq!(out.commands, 2);
    assert_eq!(count(&s), 2);
    assert_eq!(labels(&s), ["Script: test.js"]);
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(count(&s), 0);
}

#[test]
fn vc_run_returns_the_commands_result() {
    let mut s = session();
    let out = run(&mut s, &format!("const r = {RECT}; typeof r.id === 'number' && r.id > 0")).unwrap();
    assert_eq!(out.value, json!(true));
}

#[test]
fn a_throw_rolls_everything_back_and_reports_the_line() {
    let mut s = session();
    let e = run(&mut s, &format!("{RECT};\nvc.run('file.new', {{width: 50, height: 50}});\nthrow new Error('kaput');")).unwrap_err();
    assert!(e.message.contains("kaput"), "{e}");
    assert_eq!(e.line, Some(3));
    assert_eq!(e.file, "test.js");
    assert_eq!(s.documents().len(), 1);
    assert_eq!(count(&s), 0);
    assert!(labels(&s).is_empty());
    assert!(!s.in_interaction());
}

#[test]
fn a_failed_run_closes_documents_it_opened() {
    let mut s = Session::new();
    let e = run(&mut s, "vc.run('file.new', {width: 50, height: 50}); vc.run('no.such.command')").unwrap_err();
    assert!(e.message.contains("unknown command `no.such.command`"), "{e}");
    assert!(s.documents().is_empty());
}

#[test]
fn a_failing_command_can_be_caught() {
    let mut s = session();
    let out = run(&mut s, &format!("let m; try {{ vc.run('no.such.command'); }} catch (e) {{ m = e.message; }} {RECT}; m")).unwrap();
    assert!(out.value.as_str().unwrap().contains("unknown command"));
    assert_eq!(count(&s), 1);
}

#[test]
fn syntax_errors_have_a_position() {
    let mut s = session();
    let e = run(&mut s, "const a = 1;\nconst b = ;").unwrap_err();
    assert_eq!(e.line, Some(2), "{e}");
    assert!(e.column.is_some(), "{e}");
    assert!(e.to_string().starts_with("test.js:2:"), "{e}");
}

#[test]
fn runaway_loops_and_recursion_stop() {
    let mut s = session();
    let e = run(&mut s, &format!("{RECT}; while (true) {{}}")).unwrap_err();
    assert!(e.message.contains("loop"), "{e}");
    assert_eq!(count(&s), 0);
    let e = run(&mut s, "function f(n) { return f(n + 1); } f(0)").unwrap_err();
    assert!(e.message.contains("recursi") || e.message.contains("stack"), "{e}");
    let e = run(&mut s, "try { while (true) {} } catch (e) {} 'still running'").unwrap_err();
    assert!(e.message.contains("loop"), "limits can't be caught: {e}");
}

#[test]
fn nested_runaway_loops_stop() {
    let mut s = session();
    let opts = ScriptOptions { limits: Limits { instructions: 5_000_000, ..Limits::default() }, ..Default::default() };
    let e = run_script(&mut s, "for (;;) { for (let i = 0; i < 1000; i++) {} }", "nested.js", &opts).unwrap_err();
    assert!(e.message.contains("instruction"), "{e}");
}

#[test]
fn a_command_flood_stops_at_the_cap() {
    let mut s = session();
    let opts = ScriptOptions { limits: Limits { commands: 5, ..Limits::default() }, ..Default::default() };
    let e = run_script(&mut s, &format!("for (let i = 0; i < 10; i++) {RECT}"), "flood.js", &opts).unwrap_err();
    assert!(e.message.contains("more than 5 commands"), "{e}");
    assert_eq!(count(&s), 0);
}

#[test]
fn catching_the_command_cap_does_not_continue() {
    let mut s = session();
    let opts = ScriptOptions { limits: Limits { commands: 3, ..Limits::default() }, ..Default::default() };
    let src = format!("for (let i = 0; i < 10; i++) {{ try {{ {RECT}; }} catch (e) {{}} }} 'finished'");
    let e = run_script(&mut s, &src, "catcher.js", &opts).unwrap_err();
    assert!(e.message.contains("more than 3 commands"), "{e}");
    assert_eq!(count(&s), 0);
}

#[test]
fn non_json_params_are_type_errors() {
    let mut s = session();
    let e = run(&mut s, "vc.run('shape.rectangle', {x: () => 1, y: 0, width: 1, height: 1})").unwrap_err();
    assert!(e.message.contains("TypeError"), "{e}");
    let e = run(&mut s, "vc.run(42)").unwrap_err();
    assert!(e.message.contains("TypeError"), "{e}");
}

#[test]
fn document_commands_log_args_and_alert() {
    let mut s = Session::new();
    let mut args = Map::new();
    args.insert("name".into(), json!("Ada"));
    let opts = ScriptOptions { args, ..Default::default() };
    let src = "vc.log('hello', 1, {a: 2}); alert('hi ' + vc.args.name);
        ({ doc: vc.document(), shapes: vc.commands().filter(c => c.id.startsWith('shape.')).length > 3 })";
    let out = run_script(&mut s, src, "info.js", &opts).unwrap();
    assert_eq!(out.value, json!({"doc": null, "shapes": true}));
    assert_eq!(out.log, ["hello 1 {\"a\":2}"]);
    assert_eq!(out.alerts, ["hi Ada"]);
}

#[test]
fn a_read_only_script_adds_no_undo_step() {
    let mut s = session();
    let out = run(&mut s, "vc.document().artboards[0].width").unwrap();
    assert_eq!(out.value, json!(400));
    assert!(labels(&s).is_empty());
}

#[test]
fn the_completion_value_falls_back_to_text() {
    let mut s = session();
    assert_eq!(run(&mut s, "(function named() {})").unwrap().value, Value::String("function named() {}".into()));
    assert_eq!(run(&mut s, "").unwrap().value, Value::Null);
}

#[test]
fn oversize_sources_are_refused() {
    let mut s = session();
    let big = format!("/*{}*/", " ".repeat(vectorcraft_script::MAX_SOURCE_BYTES));
    assert!(run(&mut s, &big).unwrap_err().message.contains("1 MiB"));
}

#[test]
fn read_source_strips_a_bom_and_refuses_non_utf8() {
    assert_eq!(read_source(b"\xEF\xBB\xBF1 + 1").unwrap(), "1 + 1");
    assert_eq!(read_source(b"1 +\r\n1").unwrap(), "1 +\r\n1");
    assert_eq!(read_source(b"").unwrap(), "");
    assert!(read_source(b"caf\xE9").unwrap_err().contains("UTF-8"));
    assert!(read_source(&vec![b' '; vectorcraft_script::MAX_SOURCE_BYTES + 1]).unwrap_err().contains("1 MiB"));
}

#[test]
fn crlf_scripts_report_the_right_line() {
    let mut s = session();
    let e = run(&mut s, "1;\r\n2;\r\nthrow new Error('x');").unwrap_err();
    assert_eq!(e.line, Some(3), "{e}");
}

// ---------- hostile scripts (final review) ----------

#[test]
fn deeply_nested_source_is_refused_with_a_position() {
    let mut s = session();
    let deep = format!("const ok = 1;\nconst a = {}{};", "[".repeat(5000), "]".repeat(5000));
    let e = run(&mut s, &deep).unwrap_err();
    assert!(e.message.contains("nested"), "{e}");
    assert_eq!(e.line, Some(2), "{e}");
    let e = run(&mut s, &format!("x = {}1{}", "{a:".repeat(3000), "}".repeat(3000))).unwrap_err();
    assert!(e.message.contains("nested"), "{e}");
    let e = run(&mut s, &format!("{}1{}", "(".repeat(100_000), ")".repeat(100_000))).unwrap_err();
    assert!(e.message.contains("nested"), "{e}");
}

#[test]
fn very_long_expressions_are_refused() {
    let mut s = session();
    for src in [
        format!("1{}", "+1".repeat(50_000)),
        format!("{}1", "1?1:".repeat(20_000)),
        format!("{}1", "!".repeat(20_000)),
        format!("var a = {{}}; a{}", ".a".repeat(20_000)),
    ] {
        let e = run(&mut s, &src).unwrap_err();
        assert!(e.message.contains("long"), "{e}");
    }
}

#[test]
fn ordinary_nesting_and_long_data_still_run() {
    let mut s = session();
    let nested = format!("const a = {}1{}; 'ok'", "[".repeat(200), "]".repeat(200));
    assert_eq!(run(&mut s, &nested).unwrap().value, json!("ok"));
    let data = format!("const a = [{}]; a.length", vec!["1"; 100_000].join(","));
    assert_eq!(run(&mut s, &data).unwrap().value, json!(100_000));
    let sum = format!("1{}", "+1".repeat(500));
    assert_eq!(run(&mut s, &sum).unwrap().value, json!(501));
    // Brackets in strings, comments, templates and regular expressions don't count.
    let tricky = format!(
        "const s = '{}'; /* {} */ const r = /[(]{{2}}/; const t = `{}${{1}}`; s.length",
        "[".repeat(1000),
        "(".repeat(1000),
        "{".repeat(1000)
    );
    assert_eq!(run(&mut s, &tricky).unwrap().value, json!(1000));
}

#[test]
fn deep_eval_and_json_parse_are_refused() {
    let mut s = session();
    let e = run(&mut s, "eval('['.repeat(20000) + ']'.repeat(20000))").unwrap_err();
    assert!(e.message.contains("nested"), "{e}");
    let e = run(&mut s, "new Function('return ' + '('.repeat(20000) + '1' + ')'.repeat(20000))()").unwrap_err();
    assert!(e.message.contains("nested"), "{e}");
    let e = run(&mut s, "JSON.parse('['.repeat(20000) + ']'.repeat(20000))").unwrap_err();
    assert!(e.message.contains("nested"), "{e}");
    // Shallow ones still work.
    assert_eq!(run(&mut s, "eval('[[1]]')[0][0] + JSON.parse('{\"a\": [2]}').a[0]").unwrap().value, json!(3));
}

#[test]
fn values_sharing_objects_are_refused_quickly() {
    let mut s = session();
    let shared = "let o = [0]; for (let i = 0; i < 40; i++) o = [o, o];";
    let t = std::time::Instant::now();
    let e = run(&mut s, &format!("{shared} vc.run('document.inspect', {{ o }})")).unwrap_err();
    assert!(e.message.contains("TypeError"), "{e}");
    let out = run(&mut s, &format!("{shared} vc.log(o); alert(o); o")).unwrap();
    assert_eq!(out.log.len(), 1);
    assert!(t.elapsed().as_secs() < 10, "took {:?}", t.elapsed());
}

#[test]
fn an_async_throw_fails_the_run_and_rolls_back() {
    let mut s = session();
    let e = run(&mut s, &format!("async function main() {{ {RECT}; await null; throw new Error('boom'); }} main();")).unwrap_err();
    assert!(e.message.contains("boom"), "{e}");
    assert_eq!(count(&s), 0);
    let e = run(&mut s, &format!("async function main() {{ {RECT}; await null; throw new Error('later'); }} main(); 'done'")).unwrap_err();
    assert!(e.message.contains("later"), "{e}");
    assert_eq!(count(&s), 0);
    // A rejection that is handled is fine, and an awaited value is the script's value.
    let out = run(&mut s, &format!("Promise.reject(new Error('x')).catch(() => {{}}); (async () => {{ {RECT}; return 7; }})()")).unwrap();
    assert_eq!(out.value, json!(7));
    assert_eq!(count(&s), 1);
}

#[test]
fn a_limit_gives_no_misleading_position() {
    // Boa reports no place for a limit, only the calls before it: the error shouldn't point at
    // the earlier vc.run as if it were the loop.
    let mut s = session();
    let e = run(&mut s, &format!("{RECT};\nlet n = 0;\nwhile (true) n++;")).unwrap_err();
    assert!(e.message.contains("loop"), "{e}");
    assert_eq!((e.line, e.column), (None, None), "{e}");
    assert!(e.to_string().starts_with("test.js: "), "{e}");
    // A thrown string still has its place.
    let e = run(&mut s, "1;\nthrow 'plain';").unwrap_err();
    assert_eq!(e.line, Some(2), "{e}");
}
