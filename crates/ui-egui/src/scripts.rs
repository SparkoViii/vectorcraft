//! File › Scripts: run a JavaScript file over the commands ([`vectorcraft_script`]): one undo
//! step per document it changes; a failure changes nothing and is shown in a dialog with the
//! script's log. The menu lists the scripts in the Scripts folder ([`crate::Services`]'
//! `scripts_folder`; subfolders as submenus) and the ones run lately, to run them again.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;

use serde_json::{Value, json};

use crate::menus::Item;
use crate::{FilePick, VectorcraftApp, dialogs, io};

/// The files the script picker offers.
pub const EXTS: &[&str] = &["js"];

/// Recent scripts kept.
pub const MAX_RECENT: usize = 10;
/// Most scripts the Scripts folder lists, and how deep its subfolders go.
const MAX_LISTED: usize = 200;
const MAX_DEPTH: usize = 3;

/// A script or a folder of them in the Scripts folder.
#[derive(Clone, Debug, PartialEq)]
enum Listed {
    Script { name: String, path: String },
    Folder { name: String, entries: Vec<Listed> },
}

/// What File › Scripts lists, as last refreshed ([`refresh`]).
#[derive(Clone, Debug, Default, PartialEq)]
struct Listing {
    folder: Vec<Listed>,
    /// Shown at all (desktop): Open Scripts Folder is offered.
    has_folder: bool,
    recent: Vec<String>,
}

thread_local! {
    /// The menus are built on the UI thread without the app ([`crate::menus::menu_tree`]), as the
    /// Plug-ins menus read the plug-in registry.
    static LISTING: RefCell<(Listing, u64)> = RefCell::default();
    /// Menu labels live as long as the menus ([`Item`] takes `&'static str`): each name once.
    static NAMES: RefCell<HashSet<&'static str>> = RefCell::default();
}

/// Changes whenever File › Scripts' list changes: the macOS menu bar and the command palette are
/// rebuilt.
pub fn revision() -> u64 {
    LISTING.with(|l| l.borrow().1)
}

fn intern(name: &str) -> &'static str {
    NAMES.with(|n| {
        let mut n = n.borrow_mut();
        if let Some(s) = n.get(name) {
            return *s;
        }
        let s: &'static str = Box::leak(name.to_string().into_boxed_str());
        n.insert(s);
        s
    })
}

/// The scripts in `folder` (`.js` files, hidden ones aside), sorted by name, subfolders with scripts
/// in them as folders; at most [`MAX_LISTED`] in all and [`MAX_DEPTH`] levels.
fn scan(folder: &Path, depth: usize, left: &mut usize) -> Vec<Listed> {
    let Ok(dir) = std::fs::read_dir(folder) else { return vec![] };
    let mut found: Vec<(String, std::path::PathBuf, bool)> = dir
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let is_dir = e.file_type().ok()?.is_dir() || e.path().is_dir();
            (!name.starts_with('.')).then_some((name, e.path(), is_dir))
        })
        .collect();
    found.sort_by_key(|(name, ..)| name.to_lowercase());
    let mut out = vec![];
    for (name, path, is_dir) in found {
        if *left == 0 {
            break;
        }
        if is_dir {
            if depth < MAX_DEPTH {
                let entries = scan(&path, depth + 1, left);
                if !entries.is_empty() {
                    out.push(Listed::Folder { name, entries });
                }
            }
        } else if let Some(stem) = name.strip_suffix(".js").or_else(|| name.strip_suffix(".JS")) {
            *left -= 1;
            out.push(Listed::Script { name: stem.to_string(), path: path.to_string_lossy().to_string() });
        }
    }
    out
}

fn paths(listed: &[Listed], out: &mut Vec<String>) {
    for l in listed {
        match l {
            Listed::Script { path, .. } => out.push(path.clone()),
            Listed::Folder { entries, .. } => paths(entries, out),
        }
    }
}

/// The same file, whichever way its path is written (`/tmp` and `/private/tmp`).
fn same_file(a: &str, b: &str) -> bool {
    a == b || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}

/// Read the Scripts folder again and the recent scripts: File › Scripts lists them.
pub fn refresh(app: &mut VectorcraftApp) {
    let folder = app.services.scripts_folder.clone();
    let mut left = MAX_LISTED;
    let listed = folder.as_deref().map(|f| scan(Path::new(f), 1, &mut left)).unwrap_or_default();
    let mut in_folder = vec![];
    paths(&listed, &mut in_folder);
    let recent = app.ui.recent_scripts.iter().filter(|r| !in_folder.iter().any(|p| same_file(p, r))).cloned().collect();
    let listing = Listing { folder: listed, has_folder: folder.is_some(), recent };
    LISTING.with(|l| {
        let mut l = l.borrow_mut();
        if l.0 != listing {
            l.0 = listing;
            l.1 = l.1.wrapping_add(1);
        }
    });
}

fn items(listed: &[Listed]) -> Vec<Item> {
    listed
        .iter()
        .map(|l| match l {
            Listed::Script { name, path } => Item::Cmd(intern(name), "ui.runScript", json!({ "path": path })),
            Listed::Folder { name, entries } => Item::Sub(intern(name), items(entries)),
        })
        .collect()
}

/// File › Scripts: the Scripts folder's scripts, the recent ones, Check Script… and Other Script…,
/// then Open Scripts Folder and Clear Recent Scripts where they apply.
pub fn menu() -> Vec<Item> {
    LISTING.with(|l| {
        let (listing, _) = &*l.borrow();
        let mut out = items(&listing.folder);
        if !listing.recent.is_empty() {
            if !out.is_empty() {
                out.push(Item::Sep);
            }
            out.extend(
                listing
                    .recent
                    .iter()
                    .map(|p| Item::Cmd(intern(&vectorcraft_engine::cmd::fileio::file_name(p)), "ui.runScript", json!({ "path": p }))),
            );
        }
        if !out.is_empty() {
            out.push(Item::Sep);
        }
        out.push(Item::Cmd("Check Script…", "ui.checkScript", Value::Null));
        out.push(Item::Cmd("Other Script…", "ui.runScript", Value::Null));
        if listing.has_folder {
            out.push(Item::Cmd("Open Scripts Folder", "ui.openScriptsFolder", Value::Null));
        }
        if !listing.recent.is_empty() {
            out.push(Item::Cmd("Clear Recent Scripts", "ui.clearRecentScripts", Value::Null));
        }
        out
    })
}

/// Put `path` first in the recent scripts.
fn note_recent(app: &mut VectorcraftApp, path: &str) {
    let r = &mut app.ui.recent_scripts;
    r.retain(|p| p != path);
    r.insert(0, path.to_string());
    r.truncate(MAX_RECENT);
}

/// `ui.clearRecentScripts`.
pub fn clear_recent(app: &mut VectorcraftApp) -> Result<Value, String> {
    app.ui.recent_scripts.clear();
    refresh(app);
    Ok(Value::Null)
}

/// `ui.openScriptsFolder`: make the Scripts folder if it's missing and show it.
pub fn open_folder(app: &mut VectorcraftApp) -> Result<Value, String> {
    let folder = app.services.scripts_folder.clone().ok_or("there is no Scripts folder here")?;
    std::fs::create_dir_all(&folder).map_err(|e| format!("can't make {folder}: {e}"))?;
    io::open_in_app(app, &folder)?;
    refresh(app);
    Ok(Value::Null)
}

/// `ui.runScript {path?}`: run the script at `path`, else a picked one (on the web the browser's
/// picker hands it to [`drain`]).
pub fn run(app: &mut VectorcraftApp, path: Option<String>) -> Result<Value, String> {
    let path = match path {
        Some(p) => p,
        None => {
            if let Some(start) = app.services.script_async.as_mut() {
                start();
                return Ok(Value::Null);
            }
            io::pick_open(app, &FilePick { filters: vec![("JavaScript", EXTS)], ..Default::default() })?
        }
    };
    let name = vectorcraft_engine::cmd::fileio::file_name(&path);
    let bytes = match io::read(app, &path) {
        Ok(b) => b,
        // A recent script that was moved or deleted leaves the list.
        Err(e) if app.ui.recent_scripts.contains(&path) => {
            app.ui.recent_scripts.retain(|p| *p != path);
            refresh(app);
            let why = format!("{e}\n\n{path} was removed from Recent Scripts.");
            report(app, &name, &why, &[]);
            return Err(e);
        }
        Err(e) => return Err(e),
    };
    note_recent(app, &path);
    refresh(app);
    run_bytes(app, &name, &bytes)
}

/// Run the script `name` (its file name) with text `bytes` → its value. Alerts and the log are
/// shown in a dialog after the run; an error in one too.
pub fn run_bytes(app: &mut VectorcraftApp, name: &str, bytes: &[u8]) -> Result<Value, String> {
    let source = match vectorcraft_script::read_source(bytes) {
        Ok(s) => s,
        Err(e) => {
            report(app, name, &format!("{name}: {e}"), &[]);
            return Err(e);
        }
    };
    let r = vectorcraft_script::run_script(&mut app.session, &source, name, &vectorcraft_script::ScriptOptions::default());
    app.sync_views();
    match r {
        Ok(out) => {
            app.status(format!("Ran script {name} ({} commands)", out.commands));
            if !out.alerts.is_empty() || !out.log.is_empty() {
                let detail = [out.alerts.join("\n"), out.log.join("\n")].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n\n");
                let message = crate::i18n::fmt(tl!("Script “{name}” finished."), &[("name", name)]);
                dialogs::confirm::tell(app, &message, &detail);
            }
            Ok(out.value)
        }
        Err(e) => {
            let text = e.to_string();
            report(app, name, &text, &e.log);
            Err(text)
        }
    }
}

fn report(app: &mut VectorcraftApp, name: &str, error: &str, log: &[String]) {
    app.status(format!("Script {name} failed: {error}"));
    let mut detail = error.to_string();
    if !log.is_empty() {
        detail.push_str("\n\n");
        detail.push_str(&log.join("\n"));
    }
    let message = crate::i18n::fmt(tl!("Script “{name}” failed."), &[("name", name)]);
    dialogs::confirm::tell(app, &message, &detail);
}

/// Scripts picked in the browser (web): each runs as it arrives.
pub fn drain(app: &mut VectorcraftApp) {
    let arrived: Vec<(String, Vec<u8>)> =
        app.services.script_inbox.as_ref().map(|q| std::mem::take(&mut *q.lock().unwrap_or_else(|e| e.into_inner()))).unwrap_or_default();
    for (name, bytes) in arrived {
        // A failure is shown in its dialog.
        let _ = run_bytes(app, &name, &bytes);
    }
}
