//! Human-readable MIR rendering — the snapshot-test surface, the same way
//! CST dumps are the parser's.

use std::fmt::Write as _;

use crate::{
    AggregateKind, BlockId, BodyId, Const, LocalId, MirBody, MirLowered, Operand, Place, Rvalue,
    StatementKind, TerminatorKind,
};

pub fn render(lowered: &MirLowered) -> String {
    let mut out = String::new();
    for (id, body) in lowered.bodies.iter() {
        render_body(id, body, &mut out);
    }
    out
}

fn render_body(id: BodyId, body: &MirBody, out: &mut String) {
    let ret = body.return_local();
    let params = body
        .params
        .iter()
        .map(|&p| format!("{}: {}", local(p), body.locals[p].ty.display()))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = writeln!(
        out,
        "fn {}({params}) -> {} {{",
        body_name(id),
        body.locals[ret].ty.display()
    );
    for (id, data) in body.locals.iter() {
        let note = if id == ret {
            "  // return".to_owned()
        } else if body.params.contains(&id) {
            format!("  // param {}", data.name.as_deref().unwrap_or("_"))
        } else {
            match &data.name {
                Some(name) => format!("  // {name}"),
                None => String::new(),
            }
        };
        let _ = writeln!(out, "  {}: {}{note}", local(id), data.ty.display());
    }
    for (id, data) in body.blocks.iter() {
        let _ = writeln!(out, "  {}:", block(id));
        for stmt in &data.statements {
            let StatementKind::Assign { dest, rvalue } = &stmt.kind;
            let _ = writeln!(out, "    {} = {}", place(dest), render_rvalue(rvalue));
        }
        let _ = writeln!(out, "    {}", render_terminator(&data.terminator.kind));
    }
    let _ = writeln!(out, "}}");
}

fn render_rvalue(rvalue: &Rvalue) -> String {
    match rvalue {
        Rvalue::Use(op) => operand(op),
        Rvalue::BinaryOp(bin_op, l, r) => {
            format!("{bin_op:?}({}, {})", operand(l), operand(r))
        }
        Rvalue::Aggregate {
            kind: AggregateKind::Record(fields),
            ops,
        } => {
            let parts = fields
                .iter()
                .zip(ops)
                .map(|(name, op)| format!("{name}: {}", operand(op)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ {parts} }}")
        }
        // Tag-free by design, so it renders as a bare tuple — no enum, no
        // variant, nothing to leak into a snapshot.
        Rvalue::Aggregate {
            kind: AggregateKind::VariantPayload,
            ops,
        } => {
            let parts = ops.iter().map(operand).collect::<Vec<_>>().join(", ");
            format!("payload({parts})")
        }
        Rvalue::Field { base, index } => format!("{}.{index}", operand(base)),
        Rvalue::Instantiate { item, const_args } => {
            let args = const_args
                .iter()
                .map(operand)
                .collect::<Vec<_>>()
                .join(", ");
            format!("instantiate {}({args})", item.display_name())
        }
        Rvalue::WidenToEnum {
            op,
            decl,
            variant,
            index: _,
        } => format!(
            "widen {} to {}::{}",
            operand(op),
            decl.display_name(),
            variant
        ),
    }
}

fn render_terminator(kind: &TerminatorKind) -> String {
    match kind {
        TerminatorKind::Goto { target } => format!("goto -> {}", block(*target)),
        TerminatorKind::SwitchBool {
            discr,
            then_block,
            else_block,
        } => format!(
            "if {} -> [then: {}, else: {}]",
            operand(discr),
            block(*then_block),
            block(*else_block)
        ),
        TerminatorKind::SwitchVariant {
            discr,
            decl,
            arms,
            otherwise,
        } => {
            let arms = arms
                .iter()
                .map(|&(index, target)| format!("{index}: {}", block(target)))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "switch {} on {} -> [{arms}, otherwise: {}]",
                operand(discr),
                decl.display_name(),
                block(*otherwise)
            )
        }
        TerminatorKind::Call {
            callee,
            args,
            dest,
            target,
        } => {
            let args = args.iter().map(operand).collect::<Vec<_>>().join(", ");
            let target = match target {
                Some(target) => block(*target),
                None => "!".to_owned(),
            };
            format!(
                "{} = call {}({args}) -> {target}",
                local(*dest),
                operand(callee)
            )
        }
        TerminatorKind::Return => "return".to_owned(),
        TerminatorKind::Trap {
            message,
            dest,
            target,
        } => format!("{} = trap {message:?} -> {}", local(*dest), block(*target)),
        TerminatorKind::ConstTrap { message, target } => {
            format!("const trap {message:?} -> {}", block(*target))
        }
        TerminatorKind::Unreachable => "unreachable".to_owned(),
    }
}

fn operand(op: &Operand) -> String {
    match op {
        Operand::Copy(l) => local(*l),
        Operand::Const(c) => match c {
            Const::Unit => "()".to_owned(),
            Const::Int(v) => v.to_string(),
            Const::Str(s) => format!("{s:?}"),
            Const::Bool(b) => b.to_string(),
            Const::Item(loc) => format!("item {}", loc.display_name()),
            Const::Builtin(b) => format!("builtin {}", b.name()),
            Const::Fn(body) => format!("fn {}", body_name(*body)),
            Const::ConstBlock(body) => format!("const {}", body_name(*body)),
            Const::ConstParam(index) => format!("const param {index}"),
        },
    }
}

fn local(id: LocalId) -> String {
    format!("_{}", u32::from(id.into_raw()))
}

/// `_1` for a whole local, `_1.0.2` through a field-index projection —
/// the same dotted spelling `Rvalue::Field` reads render with.
fn place(p: &Place) -> String {
    let mut out = local(p.local);
    for index in &p.projection {
        let _ = write!(out, ".{index}");
    }
    out
}

fn block(id: BlockId) -> String {
    format!("bb{}", u32::from(id.into_raw()))
}

fn body_name(id: BodyId) -> String {
    format!("b{}", u32::from(id.into_raw()))
}
