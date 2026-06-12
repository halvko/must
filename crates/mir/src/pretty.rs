//! Human-readable MIR rendering — the snapshot-test surface, the same way
//! CST dumps are the parser's.

use std::fmt::Write as _;

use crate::{
    BlockId, BodyId, Const, LocalId, MirBody, MirLowered, Operand, Rvalue, StatementKind,
    TerminatorKind,
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
            let _ = writeln!(out, "    {} = {}", local(*dest), render_rvalue(rvalue));
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
        TerminatorKind::Call {
            callee,
            args,
            dest,
            target,
        } => {
            let args = args
                .iter()
                .map(|a| operand(a))
                .collect::<Vec<_>>()
                .join(", ");
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
        },
    }
}

fn local(id: LocalId) -> String {
    format!("_{}", u32::from(id.into_raw()))
}

fn block(id: BlockId) -> String {
    format!("bb{}", u32::from(id.into_raw()))
}

fn body_name(id: BodyId) -> String {
    format!("b{}", u32::from(id.into_raw()))
}
