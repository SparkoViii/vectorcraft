//! Untrusted scripts: random token soup never panics, and every command, run from a script with
//! junk params and then thrown away, leaves the documents exactly as they were.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use proptest::prelude::*;
use serde_json::json;
use vectorcraft_engine::command_specs;
use vectorcraft_script::{Limits, ScriptOptions, run_script};
use vectorcraft_testkit::catch_quiet;
use vectorcraft_testkit::fixtures::Fixture;
use vectorcraft_testkit::invariants::{check_session, doc_json};
use vectorcraft_testkit::strategies::junk_params;

fn small() -> ScriptOptions {
    ScriptOptions { limits: Limits { loop_iterations: 10_000, recursion: 64, instructions: 2_000_000, commands: 50 }, ..Default::default() }
}

const TOKENS: &[&str] = &[
    // Depth: brackets in runs, and strings that eval and JSON.parse nest.
    "[[[[[[[[[[[[[[[[",
    "((((((((((((((((",
    "{a:{a:{a:{a:",
    "`${`${`${",
    "eval(",
    "JSON.parse(",
    "'['.repeat(99999)",
    "new Function(",
    "1+1+1+1+1+1+1+1+",
    "a?b:c?d:",
    "!!!!!!!!",
    "vc.run(",
    "\"shape.rectangle\"",
    "\"no.such\"",
    "{",
    "}",
    "x:",
    "y:",
    "width:",
    "height:",
    "1",
    "-1",
    "1e308",
    "NaN",
    "0n",
    ",",
    ")",
    ";",
    "(",
    "[",
    "]",
    "while(",
    "for(;;)",
    "true",
    "function f()",
    "f()",
    "=>",
    "throw",
    "new Error('e')",
    "try{",
    "}catch(e){",
    "'a'",
    "`${",
    "`",
    "/re/g",
    "class A{}",
    "yield",
    "...",
    "?.",
    "??",
    "vc.document()",
    "vc.commands()",
    "vc.log(",
    "alert(",
    "vc.args",
    "null",
    "undefined",
    "let a=",
    "a",
    "=",
    "+",
    "Symbol()",
    "() => {}",
    "\n",
    "\r\n",
    "\u{1F600}",
    "\\",
];

proptest! {
    #![proptest_config(ProptestConfig { cases: 300, ..ProptestConfig::default() })]
    #[test]
    fn token_soup_never_panics_and_a_failed_run_changes_nothing(tokens in prop::collection::vec(prop::sample::select(TOKENS), 0..400)) {
        let src = tokens.concat();
        let mut s = Fixture::Multi.session();
        let before = doc_json(&s.doc().unwrap().doc);
        let r = catch_quiet(|| run_script(&mut s, &src, "fuzz.js", &small()));
        prop_assert!(r.is_ok(), "panic on {src:?}");
        if r.unwrap().is_err() {
            prop_assert_eq!(doc_json(&s.doc().unwrap().doc), before.clone(), "{:?}", src);
        }
        prop_assert!(!s.in_interaction());
        prop_assert!(check_session(&s).is_ok());
    }
}

/// Every command, from a script, with `{}` and its first junk params, then a throw: the documents
/// and their undo history come back exactly (view state aside).
#[test]
fn every_command_from_a_script_rolls_back() {
    let path = vectorcraft_testkit::temp_dir("script-sweep").join("out.bin").to_string_lossy().to_string();
    let mut failures = vec![];
    for c in command_specs() {
        let mut cases = vec![json!({})];
        cases.extend(junk_params(c.params, &path).into_iter().take(1));
        for p in cases {
            let mut s = Fixture::Multi.session();
            let before = (s.documents().len(), doc_json(&s.doc().unwrap().doc), s.doc().unwrap().history.undo.len());
            let src = format!("try {{ vc.run({}, {p}); }} catch (e) {{}}\nthrow new Error('undo it');", serde_json::to_string(c.id).unwrap());
            match catch_quiet(|| run_script(&mut s, &src, "sweep.js", &ScriptOptions::default())) {
                Err(msg) => failures.push(format!("PANIC {} {p}: {msg}", c.id)),
                Ok(Ok(_)) => failures.push(format!("{} {p}: the throw didn't fail the run", c.id)),
                Ok(Err(_)) => {
                    let after = (s.documents().len(), doc_json(&s.doc().unwrap().doc), s.doc().unwrap().history.undo.len());
                    // View state (the perspective grid's visibility, locks, snapping, active plane)
                    // isn't an undo step and outlasts a cancel, as zoom does.
                    if after != before && !c.params.contains("View state") {
                        failures.push(format!("{} {p}: not rolled back", c.id));
                    }
                    if let Err(e) = check_session(&s) {
                        failures.push(format!("{} {p}: {e}", c.id));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
