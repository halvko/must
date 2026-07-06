//! The paused/running program a debug session controls. Drives the eval
//! machine's step API from the session loop — sound today because Must has
//! no loops: every `continue` terminates (runaway recursion hits the frame
//! limit), so the session never needs to interrupt a running program.

use std::collections::{BTreeSet, HashMap, HashSet};
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
        let condition = make(n);
        // Arrivals count from 1: `==0` and `%0` can never be met, and a
        // breakpoint they gate would verify and then silently never stop.
        if matches!(condition, HitCondition::Eq(0) | HitCondition::Mod(0)) {
            return None;
        }
        Some(condition)
    }

    fn met(self, hits: u32) -> bool {
        match self {
            HitCondition::Eq(n) => hits == n,
            HitCondition::Ne(n) => hits != n,
            HitCondition::Gt(n) => hits > n,
            HitCondition::Ge(n) => hits >= n,
            HitCondition::Lt(n) => hits < n,
            HitCondition::Le(n) => hits <= n,
            HitCondition::Mod(n) => n != 0 && hits.is_multiple_of(n),
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

/// Machine-level identity of what the top frame executes next — stack
/// depth, frame, and the (block, statement index) its next step advances.
/// Distinct MIR statements can share a source position, so statement-level
/// movement is judged on this, not on positions.
type StepPoint = (usize, hir::ItemLoc, mir::BodyId, mir::BlockId, usize);

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
    /// Where the *live* frames have already been seen, indexed by stack
    /// depth: `sat_at[i]` is the history of `machine.frames()[i]`. This is
    /// the whole arrival record — a breakpoint fires the first time its
    /// frame is seen at its position, whenever it was installed — so a
    /// breakpoint (re)installed while paused neither re-arrives at the
    /// paused position without moving nor fires when execution unwinds
    /// back into a position the frame already arrived at.
    ///
    /// Bounded by the live stack, not by session history: a popped frame's
    /// history goes with it (deeper entries are truncated away, and an
    /// index a later call reuses is reset when the serial changes), so
    /// recursion and long stepping sessions don't accumulate.
    sat_at: Vec<FrameHistory>,
    /// Positions some MIR statement or terminator maps to (line → start
    /// columns): where breakpoints can verify, and the candidates the
    /// client's inline-breakpoint picker gets.
    executable_positions: HashMap<u32, BTreeSet<u32>>,
    /// Set when execution crashed: frames stay inspectable; the next resume
    /// terminates the session.
    pub(crate) crashed: Option<EvalError>,
    /// Compound values (records) handed out through `variables`, indexed by
    /// `variablesReference - RECORD_REF_BASE`: DAP lets a client expand a
    /// structured variable by re-requesting `variables` with the reference
    /// it was given, so a value that outlives the request that produced it
    /// needs somewhere to live. Cleared on every resume — like the paused
    /// frames themselves, these references are only meaningful for the
    /// pause that handed them out.
    record_vars: Vec<Value>,
}

/// First `variablesReference` used for a registered compound value. Frame
/// ids (the scope-level references `scopes` hands out) are small — at most
/// `MAX_FRAMES` in `eval::machine` — so this is comfortably out of range for
/// any real call stack.
const RECORD_REF_BASE: i64 = 100_000;

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
            sat_at: Default::default(),
            executable_positions,
            crashed: None,
            record_vars: Vec::new(),
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

    /// (stack depth, current (line, column) of the top frame, machine-level
    /// identity of the statement it executes next).
    fn position(&self) -> (usize, Option<(u32, u32)>, Option<StepPoint>) {
        let depth = self.machine.frames().len();
        let top = depth.checked_sub(1);
        let position = top
            .and_then(|top| self.machine.frame_origin(top))
            .and_then(|origin| {
                runner::source_position(self.db, self.file, self.original_len, &origin)
            });
        let step_point = top.and_then(|top| {
            let frame = self.machine.frames().get(top)?;
            let (block, statement) = self.machine.frame_step_point(top)?;
            Some((depth, frame.loc.clone(), frame.body, block, statement))
        });
        (depth, position, step_point)
    }

    /// Record that the top frame — `serial`, at stack depth `depth` — is
    /// at `(line, column)`, and report whether that position, and that
    /// line, is new for this frame *instance*.
    fn record(&mut self, depth: usize, serial: u64, line: u32, column: u32) -> Arrival {
        // Frames deeper than the top one have been popped; their history
        // goes with them. Padding entries (serial 0, which no real frame
        // has) cover frames that never recorded a position.
        self.sat_at.resize_with(depth, FrameHistory::default);
        let history = &mut self.sat_at[depth - 1];
        // A later call reusing this index is a different frame instance.
        if history.serial != serial {
            *history = FrameHistory {
                serial,
                ..FrameHistory::default()
            };
        }
        Arrival {
            position: history.positions.insert((line, column)),
            line: history.lines.insert(line),
        }
    }

    /// Whether a breakpoint arrives at the position just recorded: it has
    /// to match, and the frame must not have been there before — one
    /// arrival per line per frame *instance*, so a recursive call
    /// re-arrives (new frame) and unwinding back into a line does not.
    /// NOTE: revisit when the language gains loops (same frame, same line,
    /// legitimately again).
    fn arrives(bp: &BreakpointSpec, line: u32, column: u32, arrival: Arrival) -> bool {
        bp.line == line
            && match bp.column {
                Some(c) => c == column && arrival.position,
                // Line breakpoints don't distinguish columns within the line.
                None => arrival.line,
            }
    }

    pub(crate) fn resume(&mut self, mode: ResumeMode, statement_granularity: bool) -> Outcome {
        // Every variablesReference handed out for the pause we're leaving
        // becomes meaningless the moment execution moves.
        self.record_vars.clear();
        let start = self.position();
        // Line-level view of a position: what "somewhere new" means at line
        // granularity (multiple statements on one line don't re-stop).
        let line_of =
            |p: &(usize, Option<(u32, u32)>, Option<StepPoint>)| (p.0, p.1.map(|(line, _)| line));
        loop {
            let here = self.position();
            let (depth, position) = (here.0, here.1);
            // A position is the top frame's, so it implies a live frame.
            if let Some((line, column)) = position {
                let serial = self.top_serial();
                let arrival = self.record(depth, serial, line, column);
                // Breakpoint arrivals (once per line per frame instance),
                // in every resume mode. Arrival is a property of the
                // position, read by each breakpoint: a sibling's false
                // condition or log-point emission consumes nothing of its
                // neighbors'.
                for index in 0..self.breakpoints.len() {
                    if !Self::arrives(&self.breakpoints[index], line, column, arrival) {
                        continue;
                    }
                    self.breakpoints[index].hits += 1;
                    if let Some(hit_condition) = self.breakpoints[index].hit_condition
                        && !hit_condition.met(self.breakpoints[index].hits)
                    {
                        continue;
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
                    here.2 != start.2
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

    /// Registers `value` for later structured expansion and returns the
    /// `variablesReference` to hand back for it, or `0` if it has no
    /// children (DAP's convention for "not expandable" — every scalar
    /// value).
    pub(crate) fn register(&mut self, value: &Value) -> i64 {
        match value {
            // Records expand into fields, arrays into elements — same
            // registry, same reference scheme.
            Value::Record { .. } | Value::Array(_) => {
                self.record_vars.push(value.clone());
                RECORD_REF_BASE + (self.record_vars.len() as i64 - 1)
            }
            _ => 0,
        }
    }

    /// The children behind a `variablesReference`: a frame's named locals
    /// when `reference` is a frame id (as `scopes` hands out), or a
    /// previously `register`ed compound value's fields.
    pub(crate) fn dap_variables(&mut self, reference: i64) -> Vec<(String, Value)> {
        if reference >= RECORD_REF_BASE {
            let index = (reference - RECORD_REF_BASE) as usize;
            return match self.record_vars.get(index) {
                Some(Value::Record { fields }) => fields.clone(),
                // Elements named by index, like every debugger does it.
                Some(Value::Array(values)) => values
                    .iter()
                    .enumerate()
                    .map(|(i, value)| (i.to_string(), value.clone()))
                    .collect(),
                _ => Vec::new(),
            };
        }
        self.locals(reference)
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
        // Frame ids are machine indices + 1; a stale or malformed id
        // errors rather than silently evaluating against some other
        // frame.
        let frame_index = match frame_id {
            Some(id) if (1..=top).contains(&id) => id as usize - 1,
            Some(id) => return Err(format!("no frame {id}")),
            None => top.saturating_sub(1) as usize,
        };
        self.eval_in_frame(expression, frame_index)
            .map(|value| value.display())
    }

    /// The shared engine behind console evaluation, breakpoint conditions,
    /// and log-point interpolation.
    fn eval_in_frame(&mut self, expression: &str, frame_index: usize) -> Result<Value, String> {
        let expression = expression.trim();

        // A bare name reads the value the frame actually holds: innermost
        // shadow wins (later occurrences replace earlier ones), whatever
        // the binding's type — even broken (error, unresolved inference)
        // ones. The synthetic wrapper's parameters must be writable as
        // annotations, so only writable bindings join them: an un-writable
        // inner shadow doesn't evict the writable outer binding it
        // shadows, and a name with no writable binding falls back to file
        // scope.
        let mut locals: Vec<(String, hir::Ty, Value)> = Vec::new();
        let mut wrapper_params: Vec<(String, hir::Ty, Value)> = Vec::new();
        for (name, ty, value) in self.machine.frame_named_locals(frame_index) {
            locals.retain(|(existing, _, _)| existing != &name);
            locals.push((name.clone(), ty.clone(), value.clone()));
            if matches!(
                ty,
                hir::Ty::Unit
                    | hir::Ty::Int(_)
                    | hir::Ty::Str
                    | hir::Ty::Bool
                    | hir::Ty::Fn(_)
                    | hir::Ty::Record(_)
                    | hir::Ty::Array { .. }
            ) {
                wrapper_params.retain(|(existing, _, _)| existing != &name);
                wrapper_params.push((name, ty, value));
            }
        }

        if is_name(expression)
            && let Some((_, _, value)) = locals.iter().find(|(name, _, _)| name == expression)
        {
            return Ok(value.clone());
        }

        let params = wrapper_params
            .iter()
            .map(|(name, ty, _)| format!("{name}: {}", ty.display()))
            .collect::<Vec<_>>()
            .join(", ");
        let args: Vec<Value> = wrapper_params
            .into_iter()
            .map(|(_, _, value)| value)
            .collect();
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
            // Interpreter-detected UB: a deterministic stop, prefixed as
            // what it is.
            EvalErrorKind::UndefinedBehavior => "undefined behavior",
            // Unreachable in an instantiated execution; rendered
            // honestly if it ever escapes.
            EvalErrorKind::Uninstantiated => "error",
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

/// What one live frame has already been seen at. Positions and lines are
/// tracked separately because a line breakpoint arrives once per line
/// while a column breakpoint distinguishes columns within it.
#[derive(Default)]
struct FrameHistory {
    /// Frame serial; 0 is the padding value, which no real frame has
    /// (the machine's serials start at 1).
    serial: u64,
    positions: HashSet<(u32, u32)>,
    lines: HashSet<u32>,
}

/// Whether the top frame's current position is new for that frame
/// instance, at position and at line granularity.
#[derive(Clone, Copy)]
struct Arrival {
    position: bool,
    line: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Sink;

    impl Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Stepping through recursion must not accumulate history: what a
    /// popped frame sat at is gone, so the bookkeeping tracks the live
    /// stack instead of the session's own length.
    #[test]
    fn position_history_never_outlives_the_frames_it_belongs_to() {
        let path = std::env::temp_dir().join(format!("must-dap-bound-{}.must", std::process::id()));
        // Three separate descents: the session walks far more frames than
        // are ever live at once.
        std::fs::write(
            &path,
            concat!(
                "static down = fn (n: usize) -> usize {\n",
                "    if n == 0 { 0 } else { down(n - 1) }\n",
                "}\n",
                "static main = fn -> usize {\n",
                "    down(5) + down(5) + down(5)\n",
                "};\n",
            ),
        )
        .unwrap();
        let mut debuggee = Debuggee::new(path.to_str().unwrap(), "main()", Sink).unwrap();

        // The most one frame could ever have sat at: the program's own
        // executable positions.
        let program_positions: usize = debuggee
            .executable_positions
            .values()
            .map(|columns| columns.len())
            .sum();
        let program_lines = debuggee.executable_positions.len();

        let mut steps = 0;
        while let Outcome::Stopped { .. } = debuggee.resume(ResumeMode::StepIn, false) {
            steps += 1;
            assert!(steps < 1000, "the countdown should have finished by now");
            let frames = debuggee.machine.frames();
            assert_eq!(
                debuggee.sat_at.len(),
                frames.len(),
                "one history per live frame, none for popped ones"
            );
            for (history, frame) in debuggee.sat_at.iter().zip(frames) {
                // Serial 0 is the padding a frame that never recorded a
                // position leaves behind.
                assert!(
                    history.serial == 0 || history.serial == frame.serial,
                    "history {} is not the live frame's ({})",
                    history.serial,
                    frame.serial
                );
                // Bounded by the program, so the whole record is bounded
                // by the live stack — never by how long the session ran.
                assert!(history.positions.len() <= program_positions);
                assert!(history.lines.len() <= program_lines);
            }
        }
        assert!(steps > 40, "expected a long stepping session, got {steps}");

        let _ = std::fs::remove_file(path);
    }
}
