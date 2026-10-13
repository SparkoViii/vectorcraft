//! Batch runs outside a command (scripts): one undo step per document, full roll back.

use serde_json::json;

use super::*;

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

fn rect() -> Value {
    json!({"x": 0, "y": 0, "width": 10, "height": 10})
}

#[test]
fn a_batch_run_outside_a_command_is_one_undo_step() {
    let mut s = session();
    let journal = s.journal.len();
    let mut run = s.begin_batch("Script: grid.js", None).unwrap();
    s.batch_step(&mut run, "shape.rectangle", &rect()).unwrap();
    s.batch_step(&mut run, "shape.ellipse", &json!({"x": 20, "y": 0, "width": 10, "height": 10})).unwrap();
    assert_eq!(run.steps(), 2);
    s.finish_batch(run);
    assert_eq!(count(&s), 2);
    assert_eq!(labels(&s), ["Script: grid.js"]);
    assert_eq!(s.journal.len(), journal, "a batch run journals nothing itself");
    assert!(!s.in_interaction());
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(count(&s), 0);
}

#[test]
fn rolling_back_a_batch_run_restores_documents_and_session_state() {
    let mut s = session();
    let fill = s.paint.fill.label();
    let untitled = s.peek_untitled();
    let mut run = s.begin_batch("Script: x.js", None).unwrap();
    s.batch_step(&mut run, "shape.rectangle", &rect()).unwrap();
    s.batch_step(&mut run, "paint.setFill", &json!({"color": "#ff0000"})).unwrap();
    s.batch_step(&mut run, "file.new", &json!({"width": 100, "height": 100})).unwrap();
    assert_eq!(s.documents().len(), 2);
    s.roll_back_batch(run);
    assert_eq!(s.documents().len(), 1, "the document the run opened is closed");
    assert_eq!(count(&s), 0);
    assert!(labels(&s).is_empty());
    assert_eq!(s.paint.fill.label(), fill);
    assert_eq!(s.peek_untitled(), untitled);
    assert!(!s.in_interaction());
    // Commands after it run, and are journaled, as usual.
    s.execute("shape.rectangle", &rect()).unwrap();
    assert_eq!(s.journal.last().map(|(c, _)| c.as_str()), Some("shape.rectangle"));
    assert_eq!(labels(&s).len(), 1);
}

#[test]
fn rolling_back_brings_back_a_document_the_run_closed() {
    let mut s = session();
    s.execute("shape.rectangle", &rect()).unwrap();
    let mut run = s.begin_batch("Script: close.js", None).unwrap();
    s.batch_step(&mut run, "file.close", &json!({})).unwrap();
    assert!(s.documents().is_empty());
    s.roll_back_batch(run);
    assert_eq!(s.documents().len(), 1);
    assert_eq!(count(&s), 1);
}

#[test]
fn a_failed_step_returns_the_commands_own_error() {
    let mut s = session();
    let mut run = s.begin_batch("Script: a.js", None).unwrap();
    let e = s.batch_step(&mut run, "no.such.command", &json!({})).unwrap_err();
    assert_eq!(e.to_string(), "unknown command `no.such.command`");
    s.roll_back_batch(run);
}

#[test]
fn batch_runs_do_not_nest() {
    let mut s = session();
    let mut run = s.begin_batch("Script: a.js", None).unwrap();
    let e = s.batch_step(&mut run, "command.batch", &json!({"commands": []})).unwrap_err();
    assert!(e.to_string().contains("batches cannot nest"), "{e}");
    s.roll_back_batch(run);
}

#[test]
fn a_batch_run_can_start_with_no_document_open() {
    let mut s = Session::new();
    let mut run = s.begin_batch("Script: new.js", None).unwrap();
    s.batch_step(&mut run, "file.new", &json!({"width": 100, "height": 100})).unwrap();
    s.batch_step(&mut run, "shape.rectangle", &rect()).unwrap();
    s.finish_batch(run);
    assert_eq!(s.documents().len(), 1);
    assert_eq!(count(&s), 1);
    assert_eq!(labels(&s), ["Script: new.js"]);
}

#[test]
fn rolling_back_restores_a_step_the_run_undid() {
    let mut s = session();
    s.execute("shape.rectangle", &rect()).unwrap();
    let mut run = s.begin_batch("Script: undo.js", None).unwrap();
    s.batch_step(&mut run, "edit.undo", &json!({})).unwrap();
    assert_eq!(count(&s), 0);
    s.roll_back_batch(run);
    assert_eq!(count(&s), 1);
    assert_eq!(labels(&s).len(), 1);
    assert!(s.doc().unwrap().history.redo.is_empty());
}

#[test]
fn an_undo_inside_a_run_is_part_of_its_undo_step() {
    let mut s = session();
    s.execute("shape.rectangle", &rect()).unwrap();
    let mut run = s.begin_batch("Script: undo.js", None).unwrap();
    s.batch_step(&mut run, "edit.undo", &json!({})).unwrap();
    s.batch_step(&mut run, "shape.ellipse", &json!({"x": 20, "y": 0, "width": 10, "height": 10})).unwrap();
    s.finish_batch(run);
    assert_eq!(labels(&s), ["Script: undo.js"]);
    s.execute("edit.undo", &json!({})).unwrap();
    let kinds: Vec<&str> = s.doc().unwrap().doc.layers[0].children().unwrap().iter().map(|n| n.kind_label()).collect();
    assert_eq!(kinds.len(), 1, "back to the rectangle alone: {kinds:?}");
}

#[test]
fn a_failed_batch_brings_back_a_step_it_undid() {
    let mut s = session();
    s.execute("shape.rectangle", &rect()).unwrap();
    let r = s.execute("command.batch", &json!({"commands": [{"command": "edit.undo", "params": {}}, {"command": "no.such.command", "params": {}}]}));
    assert!(r.is_err());
    assert_eq!(count(&s), 1);
    assert_eq!(labels(&s).len(), 1);
    assert!(s.doc().unwrap().history.redo.is_empty());
}
