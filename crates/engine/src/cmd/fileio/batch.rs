//! `command.batch`: several commands as one undo step (in each document they edit), and the batch
//! runs behind it ([`Session::begin_batch`]), which scripts use too.

use serde_json::{Value, json};
use vectorcraft_doc::NodeId;
use vectorcraft_tools::PaintDefaults;

use super::super::*;
use crate::{Clipboard, DocState, DrawMode, EngineError, History};

/// A batch under way ([`Session::begin_batch`]): what it labels its undo steps and what an error
/// rolls back to.
pub struct BatchRun {
    label: String,
    /// The active artboard the app passes, for the steps that act on it.
    artboard: Option<u64>,
    /// Session-level state a step may change (paint defaults, drawing mode, clipboard, the
    /// Untitled-N count).
    saved: (PaintDefaults, bool, DrawMode, Option<NodeId>, Clipboard, u32),
    /// The open documents' uids in order, and the active one, before the batch.
    start: Vec<u64>,
    /// Their undo histories (a step may undo or redo).
    histories: Vec<(u64, History)>,
    active: Option<usize>,
    /// The command depth before the batch: 0 when it runs outside a command (a script), 1 inside
    /// `command.batch`.
    depth: u32,
    steps: usize,
}

impl BatchRun {
    /// The steps run so far.
    pub fn steps(&self) -> usize {
        self.steps
    }
}

impl Session {
    /// Start a batch labelled `label` (each document it edits gets one undo step with that label).
    /// Outside a command its steps run as a command's would, unjournaled. Batches don't nest.
    pub fn begin_batch(&mut self, label: &str, artboard: Option<u64>) -> Result<BatchRun> {
        // At depth 0 a stash is left over from an interrupted batch; inside a command it's ours.
        if self.depth > 0 && self.batch_stash.is_some() {
            return Err(bad("command.batch", "batches cannot nest"));
        }
        // Typing (or a drag) still in progress is kept as its own step first, as Undo does: the
        // batch is then a step of its own, and rolling it back leaves the typing.
        if self.in_interaction() {
            let typing = crate::cmd::edit::typing_in_progress(self);
            self.commit_interaction()?;
            if typing {
                self.set_tool_option("endTyping", &Value::Bool(true));
            }
        }
        let run = BatchRun {
            label: label.to_string(),
            artboard,
            saved: (self.paint.clone(), self.fill_active, self.draw_mode, self.draw_inside, self.clipboard.clone(), self.untitled_counter),
            start: self.docs.iter().map(|d| d.uid).collect(),
            histories: self.docs.iter().map(|d| (d.uid, d.history.clone())).collect(),
            active: self.active,
            depth: self.depth,
            steps: 0,
        };
        if self.depth == 0 {
            self.journal_note.clear();
            self.note_depth = 1;
            self.depth = 1;
        }
        self.batch_stash = Some(vec![]);
        Ok(run)
    }

    /// Run `id` as the next step of `run`, inside an interaction of the document active when it
    /// starts (one for all the steps in that document), so a step may open, switch or close
    /// documents. A step that acts on the active artboard ([`crate::ON_ACTIVE_ARTBOARD`]) and names
    /// none gets the run's, as the Artboards panel shows it in the document the step runs in: past
    /// that document's last artboard, the last one. → the step's result, and its params with what
    /// it took from the preferences or the clock when that differs from `params`.
    pub fn batch_step(&mut self, run: &mut BatchRun, id: &str, params: &Value) -> Result<(Value, Option<Value>)> {
        if id == "command.batch" {
            return Err(bad("command.batch", "batches cannot nest"));
        }
        if self.active().is_some() {
            self.begin_interaction(&run.label)?;
        }
        let mut params = params.clone();
        if let Some(a) = run.artboard
            && crate::ON_ACTIVE_ARTBOARD.contains(&id)
            && params.get("artboard").is_none()
            && let Some(last) = self.active().and_then(|d| d.doc.artboards.len().checked_sub(1))
            && let Some(o) = params.as_object_mut()
        {
            o.insert("artboard".into(), json!(usize::try_from(a).map_or(last, |a| a.min(last))));
        }
        let (v, noted) = self.execute_step(id, &params)?;
        run.steps += 1;
        Ok((v, (noted != params).then_some(noted)))
    }

    /// Keep what `run` did: each document it edited gets it as one undo step.
    pub fn finish_batch(&mut self, run: BatchRun) {
        self.batch_stash = None;
        self.remember_pending_paint();
        for st in &mut self.docs {
            if let Some(it) = st.interaction.take() {
                st.keep_interaction(it);
            }
        }
        self.depth = run.depth;
        if run.depth == 0 {
            // What a top-level command does after it runs.
            self.inherit_new_art();
            self.after_command();
        }
    }

    /// Undo everything `run` did: the documents it opened close, the ones it closed or replaced
    /// come back, each one's edits are undone, and the session-level state is restored.
    pub fn roll_back_batch(&mut self, run: BatchRun) {
        let stash = self.batch_stash.take().unwrap_or_default();
        roll_back(self, &run.start, stash);
        for (uid, history) in run.histories {
            if let Some(st) = self.docs.iter_mut().find(|d| d.uid == uid) {
                st.history = history;
            }
        }
        self.active = run.active;
        (self.paint, self.fill_active, self.draw_mode, self.draw_inside, self.clipboard, self.untitled_counter) = run.saved;
        self.pending_paint = None;
        // A panic may have left the depth raised (scripts catch it outside the engine).
        self.depth = run.depth;
        if run.depth == 0 {
            self.journal_note.clear();
            self.note_depth = 1;
        }
    }
}

pub(super) fn batch(s: &mut Session, p: &Value) -> Result<Value> {
    let mut cmds = p.get("commands").and_then(Value::as_array).ok_or_else(|| bad("command.batch", "missing commands"))?.clone();
    let label = str_param(p, "label").unwrap_or("Batch").to_string();
    // The active artboard the app passes, for the steps that act on it.
    let artboard = p.get("artboard").and_then(Value::as_u64);
    let mut run = s.begin_batch(&label, artboard)?;
    let mut results = Vec::with_capacity(cmds.len());
    for c in &mut cmds {
        let id = c.get("command").and_then(Value::as_str).unwrap_or("").to_string();
        let params = c.get("params").cloned().unwrap_or(json!({}));
        match s.batch_step(&mut run, &id, &params) {
            Ok((v, noted)) => {
                // The step's journal entry records what it took from the preferences or the clock
                // (a save's date, a new document's), as a command of its own would.
                if let Some(noted) = noted
                    && let Some(c) = c.as_object_mut()
                {
                    c.insert("params".into(), noted);
                }
                results.push(v);
            }
            Err(e) => {
                s.roll_back_batch(run);
                return Err(if id == "command.batch" { e } else { EngineError::Other(format!("batch step {} (`{id}`) failed: {e}", results.len())) });
            }
        }
    }
    s.finish_batch(run);
    // The batch journals itself, whichever document it ends in.
    let mut entry = p.clone();
    if let Some(o) = entry.as_object_mut() {
        o.insert("commands".into(), Value::Array(cmds));
    }
    s.journal.push(("command.batch".into(), entry));
    Ok(json!({ "results": results }))
}

/// The documents as they were before the batch (`start`: their uids in order): the ones it opened
/// close, the ones it closed or replaced (`stash`) come back, and each one's interaction is undone.
fn roll_back(s: &mut Session, start: &[u64], mut stash: Vec<DocState>) {
    let mut open = std::mem::take(&mut s.docs);
    for &uid in start {
        // The newest revision the document reached, so the restored one redraws.
        let newest = open.iter().chain(&stash).filter(|d| d.uid == uid).map(|d| d.revision).max().unwrap_or_default();
        // Its state before the batch: the first one a step closed or replaced, else the open one.
        let st = match stash.iter().position(|d| d.uid == uid) {
            Some(i) => Some(stash.remove(i)),
            None => open.iter().position(|d| d.uid == uid).map(|i| open.remove(i)),
        };
        if let Some(mut st) = st {
            st.undo_interaction();
            st.revision = newest.saturating_add(1);
            s.docs.push(st);
        }
    }
}
