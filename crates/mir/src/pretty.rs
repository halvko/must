//! Human-readable MIR rendering — the snapshot-test surface, the same way
//! CST dumps are the parser's.

use std::fmt::Write as _;

use base_db::Db;
use hir::ItemLoc;

use crate::{
    BlockId, BodyId, Const, LocalId, MirBody, MirLowered, Operand, Rvalue, StatementKind,
    TerminatorKind,
};

pub fn render(db: &dyn Db, lowered: &MirLowered) -> String {
    let mut out = String::new();
    for (id, body) in lowered.bodies.iter() {
        render_body(db, id, body, &mut out);
    }
    out
}

fn render_body(db: &dyn Db, id: BodyId, body: &MirBody, out: &mut String) {
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
            let _ = writeln!(out, "    {} = {}", local(*dest), render_rvalue(db, rvalue));
        }
        let _ = writeln!(out, "    {}", render_terminator(db, &data.terminator.kind));
    }
    let _ = writeln!(out, "}}");
}

fn render_rvalue(db: &dyn Db, rvalue: &Rvalue) -> String {
    match rvalue {
        Rvalue::Use(op) => operand(db, op),
        Rvalue::BinaryOp(bin_op, l, r) => {
            format!("{bin_op:?}({}, {})", operand(db, l), operand(db, r))
        }
    }
}

fn render_terminator(db: &dyn Db, kind: &TerminatorKind) -> String {
    match kind {
        TerminatorKind::Goto { target } => format!("goto -> {}", block(*target)),
        TerminatorKind::SwitchBool {
            discr,
            then_block,
            else_block,
        } => format!(
            "if {} -> [then: {}, else: {}]",
            operand(db, discr),
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
                .map(|a| operand(db, a))
                .collect::<Vec<_>>()
                .join(", ");
            let target = match target {
                Some(target) => block(*target),
                None => "!".to_owned(),
            };
            format!(
                "{} = call {}({args}) -> {target}",
                local(*dest),
                operand(db, callee)
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

fn operand(db: &dyn Db, op: &Operand) -> String {
    match op {
        Operand::Copy(l) => local(*l),
        Operand::Const(c) => match c {
            Const::Unit => "()".to_owned(),
            Const::Int(v) => v.to_string(),
            Const::Str(s) => format!("{s:?}"),
            Const::Bool(b) => b.to_string(),
            Const::Item(loc) => format!("item {}", item_name(db, *loc)),
            Const::Builtin(b) => format!("builtin {}", b.name()),
            Const::Fn(body) => format!("fn {}", body_name(*body)),
        },
    }
}

fn item_name(db: &dyn Db, loc: ItemLoc) -> String {
    hir::item_tree::item_tree(db, loc.file)
        .items
        .get(loc.index as usize)
        .map(|it| it.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("#{}", loc.index))
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
