//! The paused/running program a debug session controls. Drives the eval
//! machine's step API from the session loop — sound today because Must has
//! no loops: every `continue` terminates (runaway recursion hits the frame
//! limit), so the session never needs to interrupt a running program.

use std::collections::{BTreeSet, HashMap};
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

/// A client breakpoint: a line, optionally narrowed to a column (an
/// "inline" breakpoint distinguishing multiple calls on one line),
/// optionally gated by a hit count and/or a condition, optionally a log
/// point (emit and keep going instead of stopping).
#[derive(Clone)]
pub(crate) struct BreakpointSpec {
    pub(crate) line: u32,
    pub(crate) column: Option<u32>,
    pub(crate) id: i64,
    /// Stop only when this Must expression is true in the stopping frame.
    pub(crate) condition: Option<String>,
    pub(crate) hit_condition: Option<HitCondition>,
    pub(crate) log_message: Option<String>,
    /// Arrivals so far (resets when the client replaces its breakpoints).
    pub(crate) hits: u32,
    /// (frame serial, line, column-or-0) keys already arrived at: one
    /// arrival per line per frame *instance* — recursion re-arrives (new
    /// frame), unwinding back into a line does not. NOTE: revisit when the
    /// language gains loops (same frame, same line, legitimately again).
    pub(crate) visited: std::collections::HashSet<(u64, u32, u32)>,
}

/// Parsed hit-count condition. Plain `N` stops on the Nth arrival.
#[derive(Clone, Copy)]
pub(crate) enum HitCondition {
    Eq(u32),
    Ne(u32),
    Gt(u32),
    Ge(u32),
    Lt(u32),
    Le(u32),
    /// `% N`: every Nth arrival.
    Mod(u32),
}

impl HitCondition {
    pub(crate) fn parse(text: &str) -> Option<HitCondition> {
        let text = text.trim();
        let (make, rest): (fn(u32) -> HitCondition, &str) = if let Some(r) = text.strip_prefix("==")
        {
            (HitCondition::Eq, r)
        } else if let Some(r) = text.strip_prefix("!=") {
            (HitCondition::Ne, r)
        } else if let Some(r) = text.strip_prefix(">=") {
            (HitCondition::Ge, r)
        } else if let Some(r) = text.strip_prefix("<=") {
            (HitCondition::Le, r)
        } else if let Some(r) = text.strip_prefix('>') {
            (HitCondition::Gt, r)
        } else if let Some(r) = text.strip_prefix('<') {
            (HitCondition::Lt, r)
        } else if let Some(r) = text.strip_prefix('%') {
            (HitCondition::Mod, r)
        } else {
            (HitCondition::Eq, text)
        };
        let n: u32 = rest.trim().parse().ok()?;
        Some(make(n))
    }

    fn met(self, hits: u32) -> bool {
        match self {
            HitCondition::Eq(n) => hits == n,
            HitCondition::Ne(n) => hits != n,
            HitCondition::Gt(n) => hits > n,
            HitCondition::Ge(n) => hits >= n,
            HitCondition::Lt(n) => hits < n,
            HitCondition::Le(n) => hits <= n,
            HitCondition::Mod(n) => n != 0 && hits % n == 0,
        }
    }
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

pub(crate) struct Debuggee<W: Write + Clone> {
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
    /// Cloned for sub-evaluations (conditions, log points, the console).
    console: W,
    pub(crate) breakpoints: Vec<BreakpointSpec>,
    /// Positions some MIR statement or terminator maps to (line → start
    /// columns): where breakpoints can verify, and the candidates the
    /// client's inline-breakpoint picker gets.
    executable_positions: HashMap<u32, BTreeSet<u32>>,
    /// Set when execution crashed: frames stay inspectable; the next resume
    /// terminates the session.
    pub(crate) crashed: Option<EvalError>,
}

impl<W: Write + Clone> Debuggee<W> {
    /// Read and prepare `path`, ready to run `entry`. Errors are fully
    /// rendered.
    pub(crate) fn new(path: &str, entry: &str, console: W) -> Result<Debuggee<W>, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|err| format!("error: cannot read `{path}`: {err}"))?;
        let db: &'static RootDatabase = Box::leak(Box::new(RootDatabase::default()));
        let prepared = runner::prepare(db, &text, path, entry)?;
        let mut machine = Machine::new(
            db,
            RunMode {
                out: console.clone(),
            },
        );
        machine
            .start(&prepared.entry)
            .map_err(|err| format!("error: {}", err.message))?;
        let executable_positions = executable_positions(db, prepared.file, prepared.original_len);
        Ok(Debuggee {
            db,
            file: prepared.file,
            path: path.to_owned(),
            text,
            original_len: prepared.original_len,
            machine,
            console,
            breakpoints: Vec::new(),
            executable_positions,
            crashed: None,
        })
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn can_break_at(&self, line: u32, column: Option<u32>) -> bool {
        match (self.executable_positions.get(&line), column) {
            (Some(_), None) => true,
            (Some(columns), Some(column)) => columns.contains(&column),
            (None, _) => false,
        }
    }

    /// All breakpointable (line, column) positions in a line range — the
    /// candidates an inline-breakpoint picker offers.
    pub(crate) fn breakpoint_locations(&self, line: u32, end_line: u32) -> Vec<(u32, u32)> {
        let mut locations = Vec::new();
        for line in line..=end_line {
            if let Some(columns) = self.executable_positions.get(&line) {
                locations.extend(columns.iter().map(|&column| (line, column)));
            }
        }
        locations
    }

    fn top_serial(&self) -> u64 {
        self.machine.frames().last().map(|f| f.serial).unwrap_or(0)
    }

    /// (stack depth, current (line, column) of the top frame).
    fn position(&self) -> (usize, Option<(u32, u32)>) {
        let depth = self.machine.frames().len();
        let position = depth
            .checked_sub(1)
            .and_then(|top| self.machine.frame_origin(top))
            .and_then(|origin| {
                runner::source_position(self.db, self.file, self.original_len, &origin)
            });
        (depth, position)
    }

    /// The arrival key of a breakpoint at a position: `Some` when the
    /// breakpoint matches, keyed by frame instance so each frame arrives at
    /// a line once (unwinding back into the line is not a new arrival;
    /// a recursive call is).
    fn arrival_key(
        bp: &BreakpointSpec,
        serial: u64,
        position: Option<(u32, u32)>,
    ) -> Option<(u64, u32, u32)> {
        let (line, column) = position?;
        if bp.line != line {
            return None;
        }
        match bp.column {
            Some(c) if c != column => None,
            Some(_) => Some((serial, line, column)),
            // Line breakpoints don't distinguish columns within the line.
            None => Some((serial, line, 0)),
        }
    }

    pub(crate) fn resume(&mut self, mode: ResumeMode, statement_granularity: bool) -> Outcome {
        let start = self.position();
        // Line-level view of a position: what "somewhere new" means at line
        // granularity (multiple statements on one line don't re-stop).
        let line_of = |p: &(usize, Option<(u32, u32)>)| (p.0, p.1.map(|(line, _)| line));
        // Seed the resume point as visited, so resuming from a stop at a
        // breakpoint (or adding one at the paused line) doesn't re-arrive
        // without moving.
        let start_serial = self.top_serial();
        for bp in &mut self.breakpoints {
            if let Some(key) = Self::arrival_key(bp, start_serial, start.1) {
                bp.visited.insert(key);
            }
        }
        loop {
            let here = self.position();
            let (depth, position) = here;
            if depth > 0 && position.is_some() {
                let serial = self.top_serial();
                // Breakpoint arrivals (once per line per frame instance),
                // in every resume mode.
                for index in 0..self.breakpoints.len() {
                    let arrived = Self::arrival_key(&self.breakpoints[index], serial, position)
                        .is_some_and(|key| self.breakpoints[index].visited.insert(key));
                    if !arrived {
                        continue;
                    }
                    self.breakpoints[index].hits += 1;
                    if let Some(hit_condition) = self.breakpoints[index].hit_condition {
                        if !hit_condition.met(self.breakpoints[index].hits) {
                            continue;
                        }
                    }
                    if let Some(condition) = self.breakpoints[index].condition.clone() {
                        let top = self.machine.frames().len() - 1;
                        match self.eval_in_frame(&condition, top) {
                            Ok(Value::Bool(true)) => {}
                            Ok(Value::Bool(false)) => continue,
                            // A broken condition must be noticed, not
                            // silently skipped: warn and stop.
                            Ok(other) => self.console_line(&format!(
                                "warning: breakpoint condition `{condition}` is not a bool (got {})",
                                other.display()
                            )),
                            Err(message) => self.console_line(&format!(
                                "warning: breakpoint condition `{condition}` failed: {message}"
                            )),
                        }
                    }
                    if let Some(log_message) = self.breakpoints[index].log_message.clone() {
                        // A log point: emit and keep going.
                        let top = self.machine.frames().len() - 1;
                        let text = self.interpolate(&log_message, top);
                        self.console_line(&text);
                        continue;
                    }
                    return Outcome::Stopped {
                        reason: "breakpoint",
                        hit_breakpoint: Some(self.breakpoints[index].id),
                    };
                }
                let moved = if statement_granularity {
                    here != start
                } else {
                    line_of(&here) != line_of(&start)
                };
                let (stop, reason) = match mode {
                    ResumeMode::Entry => (true, "entry"),
                    ResumeMode::Continue => (false, ""),
                    ResumeMode::StepOver => {
                        (depth < start.0 || (depth == start.0 && moved), "step")
                    }
                    ResumeMode::StepIn => (moved || depth != start.0, "step"),
                    ResumeMode::StepOut => (depth < start.0, "step"),
                };
                if stop {
                    return Outcome::Stopped {
                        reason,
                        hit_breakpoint: None,
                    };
                }
            }
            match self.machine.step() {
                Ok(StepEvent::Progress) => {}
                Ok(StepEvent::Done(value)) => return Outcome::Done(value),
                Err(err) => return Outcome::Crashed(err),
            }
        }
    }

    /// Substitute `{expr}` pieces of a log point's message with their
    /// values, evaluated in `frame_index`.
    fn interpolate(&mut self, template: &str, frame_index: usize) -> String {
        let mut out = String::new();
        let mut rest = template;
        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let Some(close) = rest[open..].find('}') else {
                out.push_str(&rest[open..]);
                return out;
            };
            let expr = &rest[open + 1..open + close];
            match self.eval_in_frame(expr, frame_index) {
                Ok(value) => out.push_str(&value.display()),
                Err(message) => out.push_str(&format!("{{error: {message}}}")),
            }
            rest = &rest[open + close + 1..];
        }
        out.push_str(rest);
        out
    }

    fn console_line(&mut self, text: &str) {
        let mut console = self.console.clone();
        let _ = writeln!(console, "{text}");
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
    ) -> Result<String, String> {
        let top = self.machine.frames().len() as i64;
        let frame_index = usize::try_from(frame_id.unwrap_or(top) - 1).unwrap_or(0);
        self.eval_in_frame(expression, frame_index)
            .map(|value| value.display())
    }

    /// The shared engine behind console evaluation, breakpoint conditions,
    /// and log-point interpolation.
    fn eval_in_frame(&mut self, expression: &str, frame_index: usize) -> Result<Value, String> {
        let expression = expression.trim();

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
                return Ok(value.clone());
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

        let mut machine = Machine::new(
            self.db,
            RunMode {
                out: self.console.clone(),
            },
        );
        machine
            .eval_root(&prepared.entry)
            .and_then(|fn_value| match fn_value {
                Value::Fn(f) => machine.call_value(f, args),
                other => Ok(other),
            })
            .map_err(|err| self.render_error(&err))
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
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// Every user-file (line → start columns) some MIR statement or terminator
/// maps to.
fn executable_positions(
    db: &RootDatabase,
    file: SourceFile,
    original_len: usize,
) -> HashMap<u32, BTreeSet<u32>> {
    let mut positions: HashMap<u32, BTreeSet<u32>> = HashMap::new();
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
                    if let Some((line, column)) =
                        runner::source_position(db, file, original_len, &(loc.clone(), origin))
                    {
                        positions.entry(line).or_default().insert(column);
                    }
                }
            }
        }
    }
    positions
}
