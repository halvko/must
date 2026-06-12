//! The paused/running program a debug session controls. Drives the eval
//! machine's step API from the session loop — sound today because Must has
//! no loops: every `continue` terminates (runaway recursion hits the frame
//! limit), so the session never needs to interrupt a running program.

use std::collections::{HashMap, HashSet};
use std::io::Write;

use base_db::{RootDatabase, SourceFile};
use eval::{EvalError, EvalErrorKind, Machine, RunMode, StepEvent, Value};

use crate::runner;

pub(crate) enum ResumeMode {
    /// Run to the first position in the user's file (stop-on-entry).
    Entry,
    /// Run until a breakpoint (or the end).
    Continue,
    StepOver,
    StepIn,
    StepOut,
}

pub(crate) enum Outcome {
    Stopped {
        reason: &'static str,
        hit_breakpoint: Option<i64>,
    },
    Done(Value),
    Crashed(EvalError),
}

pub(crate) struct StackFrame {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) line: u32,
    pub(crate) column: u32,
}

pub(crate) struct Debuggee<W: Write> {
    /// The database must outlive the machine borrowing it; a debug session
    /// owns its process (it exits on disconnect), so leaking one database
    /// for the session's lifetime is the simple, honest option.
    db: &'static RootDatabase,
    file: SourceFile,
    path: String,
    /// The user's text, before entry injection (console evaluations inject
    /// into fresh copies).
    text: String,
    original_len: usize,
    machine: Machine<'static, RunMode<W>>,
    /// Source line (1-based) → breakpoint id.
    pub(crate) breakpoints: HashMap<u32, i64>,
    /// Lines some MIR statement or terminator maps to: where a breakpoint
    /// can verify.
    executable_lines: HashSet<u32>,
    /// Set when execution crashed: frames stay inspectable; the next resume
    /// terminates the session.
    pub(crate) crashed: Option<EvalError>,
}

impl<W: Write> Debuggee<W> {
    /// Read and prepare `path`, ready to run `entry`. Errors are fully
    /// rendered.
    pub(crate) fn new(path: &str, entry: &str, console: W) -> Result<Debuggee<W>, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|err| format!("error: cannot read `{path}`: {err}"))?;
        let db: &'static RootDatabase = Box::leak(Box::new(RootDatabase::default()));
        let prepared = runner::prepare(db, &text, path, entry)?;
        let mut machine = Machine::new(db, RunMode { out: console });
        machine
            .start(&prepared.entry)
            .map_err(|err| format!("error: {}", err.message))?;
        let executable_lines = executable_lines(db, prepared.file, prepared.original_len);
        Ok(Debuggee {
            db,
            file: prepared.file,
            path: path.to_owned(),
            text,
            original_len: prepared.original_len,
            machine,
            breakpoints: HashMap::new(),
            executable_lines,
            crashed: None,
        })
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn can_break_at(&self, line: u32) -> bool {
        self.executable_lines.contains(&line)
    }

    /// (stack depth, current source line of the top frame).
    fn position(&self) -> (usize, Option<u32>) {
        let depth = self.machine.frames().len();
        let line = depth
            .checked_sub(1)
            .and_then(|top| self.machine.frame_origin(top))
            .and_then(|origin| {
                runner::source_position(self.db, self.file, self.original_len, &origin)
            })
            .map(|(line, _)| line);
        (depth, line)
    }

    pub(crate) fn resume(&mut self, mode: ResumeMode) -> Outcome {
        let start = self.position();
        loop {
            let here = self.position();
            let (depth, line) = here;
            if depth > 0 {
                if let Some(line) = line {
                    // Breakpoints apply in every mode — but not at the spot
                    // we're resuming from.
                    if here != start {
                        if let Some(&id) = self.breakpoints.get(&line) {
                            return Outcome::Stopped {
                                reason: "breakpoint",
                                hit_breakpoint: Some(id),
                            };
                        }
                    }
                    let (stop, reason) = match mode {
                        ResumeMode::Entry => (true, "entry"),
                        ResumeMode::Continue => (false, ""),
                        ResumeMode::StepOver => (
                            depth < start.0 || (depth == start.0 && here != start),
                            "step",
                        ),
                        ResumeMode::StepIn => (here != start, "step"),
                        ResumeMode::StepOut => (depth < start.0, "step"),
                    };
                    if stop {
                        return Outcome::Stopped {
                            reason,
                            hit_breakpoint: None,
                        };
                    }
                } else if matches!(mode, ResumeMode::StepOut) && depth < start.0 {
                    // Stepped out into synthetic code (the entry frame):
                    // keep going until something user-visible or the end.
                }
            }
            match self.machine.step() {
                Ok(StepEvent::Progress) => {}
                Ok(StepEvent::Done(value)) => return Outcome::Done(value),
                Err(err) => return Outcome::Crashed(err),
            }
        }
    }

    /// The visible stack, top frame first. Frames without a user-source
    /// position (the synthetic entry) are omitted; ids stay tied to machine
    /// frame indices so scopes/variables resolve correctly.
    pub(crate) fn stack_frames(&self) -> Vec<StackFrame> {
        let frames = self.machine.frames();
        (0..frames.len())
            .rev()
            .filter_map(|index| {
                let origin = self.machine.frame_origin(index)?;
                let (line, column) =
                    runner::source_position(self.db, self.file, self.original_len, &origin)?;
                Some(StackFrame {
                    id: index as i64 + 1,
                    name: frames[index].loc.display_name().to_owned(),
                    line,
                    column,
                })
            })
            .collect()
    }

    /// Named locals of a DAP frame id, innermost shadowing winning is the
    /// caller's concern (names may repeat, in declaration order).
    pub(crate) fn locals(&self, frame_id: i64) -> Vec<(String, Value)> {
        let Ok(index) = usize::try_from(frame_id - 1) else {
            return Vec::new();
        };
        self.machine
            .frame_named_locals(index)
            .into_iter()
            .map(|(name, _, value)| (name, value))
            .collect()
    }

    /// Console evaluation, against the selected frame: a bare name reads a
    /// local directly; any other expression is wrapped in a synthetic fn
    /// whose parameters are the frame's locals, which the machine then
    /// calls with the actual runtime values — so `n + 1` works while paused
    /// inside a frame that has `n`. Side effects included; it's a repl.
    ///
    /// This is read-only by construction: the values are *copied in*. When
    /// the language gains assignment, console mutation (`n = 21`) needs the
    /// values copied back out — sound as in-place mutation for as long as
    /// Must values stay value-semantic (no reference identity), after which
    /// eval frames must genuinely alias the paused frame's slots.
    pub(crate) fn evaluate(
        &mut self,
        expression: &str,
        frame_id: Option<i64>,
        console: W,
    ) -> Result<String, String> {
        let expression = expression.trim();
        let top = self.machine.frames().len() as i64;
        let frame_index = usize::try_from(frame_id.unwrap_or(top) - 1).unwrap_or(0);

        // Innermost shadow wins, so later occurrences replace earlier ones;
        // locals whose type can't be written as an annotation (error,
        // unresolved inference) can't become parameters and are dropped.
        let mut locals: Vec<(String, hir::Ty, Value)> = Vec::new();
        for (name, ty, value) in self.machine.frame_named_locals(frame_index) {
            locals.retain(|(existing, _, _)| existing != &name);
            if matches!(
                ty,
                hir::Ty::Unit | hir::Ty::Int | hir::Ty::Str | hir::Ty::Bool | hir::Ty::Fn(_)
            ) {
                locals.push((name, ty, value));
            }
        }

        if is_name(expression) {
            if let Some((_, _, value)) = locals.iter().find(|(name, _, _)| name == expression) {
                return Ok(value.display());
            }
        }

        let params = locals
            .iter()
            .map(|(name, ty, _)| format!("{name}: {}", ty.display()))
            .collect::<Vec<_>>()
            .join(", ");
        let args: Vec<Value> = locals.into_iter().map(|(_, _, value)| value).collect();
        // The newline keeps a trailing line comment in the expression from
        // eating the closing brace.
        let wrapped = format!("fn ({params}) {{ {expression}\n}}");
        let prepared = runner::prepare(self.db, &self.text, &self.path, &wrapped)?;

        let mut machine = Machine::new(self.db, RunMode { out: console });
        let result = machine
            .eval_root(&prepared.entry)
            .and_then(|fn_value| match fn_value {
                Value::Fn(f) => machine.call_value(f, args),
                other => Ok(other),
            });
        match result {
            Ok(value) => Ok(value.display()),
            Err(err) => Err(self.render_error(&err)),
        }
    }

    /// Render an eval error the way the CLI runner does: kind prefix,
    /// message, and a source location when one is known.
    pub(crate) fn render_error(&self, err: &EvalError) -> String {
        let prefix = match err.kind {
            EvalErrorKind::Trap => "error",
            EvalErrorKind::Panic => "panicked",
            EvalErrorKind::Runtime => "runtime error",
            EvalErrorKind::NotConst => "error",
        };
        let mut rendered = format!("{prefix}: {}", err.message);
        if let Some(origin) = &err.origin {
            match runner::source_position(self.db, self.file, self.original_len, origin) {
                Some((line, column)) => {
                    rendered.push_str(&format!("\n  --> {}:{line}:{column}", self.path));
                }
                None => rendered.push_str("\n  --> entry expression"),
            }
        }
        rendered
    }
}

fn is_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Every user-file line some MIR statement or terminator maps to.
fn executable_lines(db: &RootDatabase, file: SourceFile, original_len: usize) -> HashSet<u32> {
    let mut lines = HashSet::new();
    for &item in hir::file_item_ids(db, file) {
        let loc = hir::item_loc(db, item);
        let lowered = mir::mir_lowered(db, item);
        for (_, body) in lowered.bodies.iter() {
            for (_, block) in body.blocks.iter() {
                let origins = block
                    .statements
                    .iter()
                    .map(|s| s.origin)
                    .chain([block.terminator.origin]);
                for origin in origins {
                    if let Some((line, _)) = runner::source_position(
                        db,
                        file,
                        original_len,
                        &(loc.clone(), origin),
                    ) {
                        lines.insert(line);
                    }
                }
            }
        }
    }
    lines
}
