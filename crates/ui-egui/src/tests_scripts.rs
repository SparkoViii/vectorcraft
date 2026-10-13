//! File › Scripts › Other Script…: the menu item, a picked script's run as one undo step, the
//! error and alert dialogs, and scripts picked in the browser.

use std::sync::{Arc, Mutex};

use serde_json::json;
use vectorcraft_engine::Session;

use crate::{VectorcraftApp, dialogs, menus, scripts, theme};

fn app() -> VectorcraftApp {
    let mut app = VectorcraftApp::new(Session::new(), Default::default());
    app.run("file.new", json!({"width": 200, "height": 200})).unwrap();
    app.services.read = Some(Box::new(|p: &str| std::fs::read(p).map_err(|e| e.to_string())));
    app
}

fn script(name: &str, src: &str) -> String {
    let dir = std::env::temp_dir().join(format!("vectorcraft-ui-scripts-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, src).unwrap();
    p.to_string_lossy().to_string()
}

fn count(app: &VectorcraftApp) -> usize {
    app.session.doc().unwrap().doc.layers[0].children().unwrap().len()
}

/// One headless frame of the dialog layer.
fn frame(app: &mut VectorcraftApp) {
    let ctx = egui::Context::default();
    theme::install_fonts(&ctx);
    theme::apply(&ctx, Default::default());
    let mut out = ctx.run_ui(Default::default(), |ui| dialogs::show(app, ui.ctx()));
    out.textures_delta.clear();
}

#[test]
fn other_script_is_in_the_file_menu_with_its_shortcut() {
    let app = app();
    let entries = menus::menu_entries(&app);
    let e = entries.iter().find(|e| e.path == ["File", "Scripts"] && e.label == "Other Script…").expect("listed");
    assert_eq!(e.command.as_deref(), Some("ui.runScript"));
    assert!(e.enabled);
    assert!(e.shortcut.contains("F12"), "{}", e.shortcut);
}

#[test]
fn a_picked_script_runs_as_one_undo_step() {
    let mut app = app();
    let path = script(
        "draw.js",
        "vc.run('shape.rectangle', {x: 10, y: 10, width: 50, height: 50});\nvc.run('shape.ellipse', {x: 80, y: 10, width: 50, height: 50});",
    );
    app.services.pick_open = Some(Box::new(move |_: &crate::FilePick| Some(path.clone())));
    app.run("ui.runScript", json!({})).unwrap();
    assert_eq!(count(&app), 2);
    let labels: Vec<String> = app.session.doc().unwrap().history.undo.iter().map(|h| h.label.clone()).collect();
    assert_eq!(labels, ["Script: draw.js"]);
    assert!(app.ui.dialog.is_none(), "a quiet script shows no dialog");
}

#[test]
fn a_failing_script_shows_the_error_and_changes_nothing() {
    let mut app = app();
    let path = script("boom.js", "vc.run('shape.rectangle', {x: 10, y: 10, width: 50, height: 50});\nthrow new Error('kaput');");
    assert!(app.run("ui.runScript", json!({"path": path})).is_err());
    assert_eq!(count(&app), 0);
    let d = app.ui.dialog.as_ref().expect("error dialog");
    assert!(d.str("message").contains("boom.js"), "{}", d.str("message"));
    assert!(d.str("detail").contains("boom.js:2:") && d.str("detail").contains("kaput"), "{}", d.str("detail"));
    frame(&mut app);
}

#[test]
fn alerts_and_the_log_are_shown_after_the_run() {
    let mut app = app();
    let path = script("hello.js", "vc.log('logged'); alert('hello there');");
    app.run("ui.runScript", json!({"path": path})).unwrap();
    let d = app.ui.dialog.as_ref().expect("dialog");
    assert!(d.str("detail").contains("hello there") && d.str("detail").contains("logged"), "{}", d.str("detail"));
    frame(&mut app);
}

#[test]
fn scripts_picked_in_the_browser_run_from_the_inbox() {
    let mut app = app();
    let inbox: crate::Inbox = Arc::new(Mutex::new(vec![]));
    let started = Arc::new(Mutex::new(0));
    let s2 = started.clone();
    app.services.script_async = Some(Box::new(move || *s2.lock().unwrap() += 1));
    app.services.script_inbox = Some(inbox.clone());
    app.run("ui.runScript", json!({})).unwrap();
    assert_eq!(*started.lock().unwrap(), 1, "the browser's picker opened");
    inbox.lock().unwrap().push(("web.js".into(), b"vc.run('shape.rectangle', {x: 1, y: 1, width: 5, height: 5})".to_vec()));
    scripts::drain(&mut app);
    assert_eq!(count(&app), 1);
}

#[test]
fn a_script_needs_no_open_document() {
    let mut app = VectorcraftApp::new(Session::new(), Default::default());
    app.services.read = Some(Box::new(|p: &str| std::fs::read(p).map_err(|e| e.to_string())));
    let path = script("new.js", "vc.run('file.new', {width: 100, height: 100}); vc.run('shape.rectangle', {x: 1, y: 1, width: 5, height: 5});");
    app.run("ui.runScript", json!({"path": path})).unwrap();
    assert_eq!(count(&app), 1);
}

// ---------- File › Scripts lists scripts to run again ----------

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("vectorcraft-ui-scripts-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

const DRAW: &str = "vc.run('shape.rectangle', {x: 1, y: 1, width: 5, height: 5})";

/// The File › Scripts entries (path under Scripts, label, the script they run).
fn script_entries(app: &VectorcraftApp) -> Vec<(Vec<String>, String, Option<String>)> {
    menus::menu_entries(app)
        .into_iter()
        .filter(|e| e.path.len() >= 2 && e.path[..2] == ["File", "Scripts"])
        .map(|e| (e.path[2..].to_vec(), e.label, e.params["path"].as_str().map(str::to_string)))
        .collect()
}

fn labels(app: &VectorcraftApp) -> Vec<String> {
    script_entries(app).into_iter().map(|(_, l, _)| l).collect()
}

#[test]
fn scripts_in_the_scripts_folder_are_listed_with_subfolders_as_submenus() {
    let dir = temp_dir("folder");
    std::fs::write(dir.join("Flower.js"), DRAW).unwrap();
    std::fs::write(dir.join("badges.js"), DRAW).unwrap();
    std::fs::write(dir.join("notes.txt"), "not a script").unwrap();
    std::fs::write(dir.join(".hidden.js"), DRAW).unwrap();
    std::fs::create_dir(dir.join("Tools")).unwrap();
    std::fs::write(dir.join("Tools").join("Report.js"), DRAW).unwrap();
    let mut app = app();
    app.services.scripts_folder = Some(dir.to_string_lossy().to_string());
    scripts::refresh(&mut app);
    let entries = script_entries(&app);
    assert_eq!(entries[0].1, "badges");
    assert_eq!(entries[1].1, "Flower");
    assert!(entries.iter().any(|(sub, l, p)| sub == &["Tools"] && l == "Report" && p.as_deref().is_some_and(|p| p.ends_with("Report.js"))));
    let all = labels(&app);
    assert!(!all.iter().any(|l| l.contains("notes") || l.contains("hidden")), "{all:?}");
    assert!(all.iter().any(|l| l == "Other Script…") && all.iter().any(|l| l == "Open Scripts Folder"), "{all:?}");
}

#[test]
fn a_listed_script_runs_from_the_menu() {
    let dir = temp_dir("run");
    std::fs::write(dir.join("Box.js"), DRAW).unwrap();
    let mut app = app();
    app.services.scripts_folder = Some(dir.to_string_lossy().to_string());
    scripts::refresh(&mut app);
    let e = menus::menu_entries(&app).into_iter().find(|e| e.label == "Box").expect("listed");
    app.run(e.command.as_deref().unwrap(), e.params.clone()).unwrap();
    assert_eq!(count(&app), 1);
}

#[test]
fn scripts_run_become_recent_newest_first_ten_at_most_once_each() {
    let dir = temp_dir("recent");
    let mut app = app();
    let paths: Vec<String> = (0..12)
        .map(|i| {
            let p = dir.join(format!("s{i}.js"));
            std::fs::write(&p, DRAW).unwrap();
            p.to_string_lossy().to_string()
        })
        .collect();
    for p in &paths {
        app.run("ui.runScript", json!({"path": p})).unwrap();
    }
    app.run("ui.runScript", json!({"path": paths[5]})).unwrap();
    assert_eq!(app.ui.recent_scripts.len(), 10);
    assert_eq!(app.ui.recent_scripts[0], paths[5]);
    assert_eq!(app.ui.recent_scripts[1], paths[11]);
    assert_eq!(app.ui.recent_scripts.iter().filter(|p| **p == paths[5]).count(), 1);
    let all = labels(&app);
    assert_eq!(all[0], "s5.js", "{all:?}");
    assert!(!all.contains(&"s0.js".to_string()), "the oldest dropped: {all:?}");
}

#[test]
fn a_folder_script_is_not_listed_again_as_recent() {
    let dir = temp_dir("dedupe");
    std::fs::write(dir.join("Box.js"), DRAW).unwrap();
    let mut app = app();
    app.services.scripts_folder = Some(dir.to_string_lossy().to_string());
    app.run("ui.runScript", json!({"path": dir.join("Box.js").to_string_lossy()})).unwrap();
    let all = labels(&app);
    assert_eq!(all.iter().filter(|l| l.starts_with("Box")).count(), 1, "{all:?}");
}

#[test]
fn a_recent_script_that_is_gone_is_dropped_with_a_message() {
    let dir = temp_dir("gone");
    let p = dir.join("Gone.js");
    std::fs::write(&p, DRAW).unwrap();
    let mut app = app();
    let path = p.to_string_lossy().to_string();
    app.run("ui.runScript", json!({"path": path})).unwrap();
    std::fs::remove_file(&p).unwrap();
    assert!(app.run("ui.runScript", json!({"path": path})).is_err());
    assert!(app.ui.recent_scripts.is_empty());
    assert!(app.ui.dialog.as_ref().is_some_and(|d| d.str("detail").contains("Recent Scripts")));
    assert!(!labels(&app).contains(&"Gone.js".to_string()));
}

#[test]
fn clear_recent_scripts_empties_the_list() {
    let dir = temp_dir("clear");
    let p = dir.join("One.js");
    std::fs::write(&p, DRAW).unwrap();
    let mut app = app();
    app.run("ui.runScript", json!({"path": p.to_string_lossy()})).unwrap();
    assert!(labels(&app).contains(&"One.js".to_string()));
    let before = scripts::revision();
    app.run("ui.clearRecentScripts", json!({})).unwrap();
    assert!(app.ui.recent_scripts.is_empty());
    assert!(!labels(&app).contains(&"One.js".to_string()));
    assert_ne!(scripts::revision(), before, "the native menu is rebuilt");
}

#[test]
fn without_a_scripts_folder_only_check_script_and_other_script_are_listed() {
    let mut app = app();
    scripts::refresh(&mut app);
    assert_eq!(labels(&app), ["Check Script…", "Other Script…"]);
}

#[test]
fn open_scripts_folder_makes_it_and_shows_it() {
    let dir = temp_dir("open").join("VectorCraft Scripts");
    let mut app = app();
    app.services.scripts_folder = Some(dir.to_string_lossy().to_string());
    let opened = Arc::new(Mutex::new(vec![]));
    let o = opened.clone();
    app.services.open_file = Some(Box::new(move |p: &str| {
        o.lock().unwrap().push(p.to_string());
        Ok(())
    }));
    app.run("ui.openScriptsFolder", json!({})).unwrap();
    assert!(dir.is_dir());
    assert_eq!(*opened.lock().unwrap(), [dir.to_string_lossy().to_string()]);
}

#[test]
fn only_other_script_carries_the_shortcut() {
    let dir = temp_dir("shortcut");
    std::fs::write(dir.join("Box.js"), DRAW).unwrap();
    let mut app = app();
    app.services.scripts_folder = Some(dir.to_string_lossy().to_string());
    app.run("ui.runScript", json!({"path": dir.join("Box.js").to_string_lossy()})).unwrap();
    let with_key: Vec<String> = menus::menu_entries(&app)
        .into_iter()
        .filter(|e| e.path.len() >= 2 && e.path[..2] == ["File", "Scripts"] && !e.shortcut.is_empty())
        .map(|e| e.label)
        .collect();
    assert_eq!(with_key, ["Other Script…"]);
}
