//! Human-readable MIR rendering — the snapshot-test surface, the same way
//! CST dumps are the parser's.

use std::fmt::Write as _;

use crate::{
    AggregateKind, BlockId, BodyId, Const, LocalId, MirBody, MirLowered, Operand, Place, ProjElem,
    StatementKind, TerminatorKind, ValueExpr,
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
            match &stmt.kind {
                StatementKind::Assign { dest, value } => {
                    let _ = writeln!(out, "    {} = {}", place(dest), render_value_expr(value));
                }
                StatementKind::StorageDead { local: id, .. } => {
                    let _ = writeln!(out, "    storage_dead {}", local(*id));
                }
            }
        }
        let _ = writeln!(out, "    {}", render_terminator(&data.terminator.kind));
    }
    let _ = writeln!(out, "}}");
}

fn render_value_expr(value: &ValueExpr) -> String {
    match value {
        ValueExpr::Use(op) => operand(op),
        ValueExpr::BinaryOp(bin_op, l, r) => {
            format!("{bin_op:?}({}, {})", operand(l), operand(r))
        }
        ValueExpr::UnaryNeg(op) => format!("Neg({})", operand(op)),
        ValueExpr::Aggregate {
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
        // Spelled like the constructor call that built it.
        ValueExpr::Aggregate {
            kind: AggregateKind::Variant { decl, variant, .. },
            ops,
        } => {
            let name = format!("{}::{variant}", decl.display_name());
            if ops.is_empty() {
                name
            } else {
                let parts = ops.iter().map(operand).collect::<Vec<_>>().join(", ");
                format!("{name}({parts})")
            }
        }
        ValueExpr::Aggregate {
            kind: AggregateKind::Array,
            ops,
        } => {
            let parts = ops.iter().map(operand).collect::<Vec<_>>().join(", ");
            format!("[{parts}]")
        }
        ValueExpr::Field { base, index } => format!("{}.{index}", operand(base)),
        ValueExpr::Index { base, index } => format!("{}[{}]", operand(base), operand(index)),
        ValueExpr::Repeat { elem, count } => format!("[{}; {}]", operand(elem), operand(count)),
        ValueExpr::AddrOf { mutable, place: p } => {
            let flavor = if *mutable { "&raw mut" } else { "&raw" };
            format!("{}.{flavor}", place(p))
        }
        // Rendered without a region: MIR is region-erased, and a snapshot
        // that showed one would be advertising information this layer does
        // not have.
        ValueExpr::Borrow { mutable, place: p } => {
            let flavor = if *mutable { "&mut" } else { "&" };
            format!("{}.{flavor}", place(p))
        }
        ValueExpr::AddrOfStatic { item, projection } => {
            let mut path = format!("static {}", item.display_name());
            path.push_str(&projection_suffix(projection));
            path.push_str(".&raw");
            path
        }
        // `closure fn#1(x, &mut y)`: the places each call borrows, with
        // the flavor it borrows them in.
        ValueExpr::Closure { body, captures } => {
            let places = captures
                .iter()
                .map(|capture| {
                    let flavor = if capture.mutable { "&mut " } else { "&" };
                    format!("{flavor}{}", place(&capture.place))
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("closure {}({places})", body_name(*body))
        }
        ValueExpr::Instantiate { item, const_args } => {
            let args = const_args
                .iter()
                .map(operand)
                .collect::<Vec<_>>()
                .join(", ");
            format!("instantiate {}({args})", item.display_name())
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
        Operand::Copy(p) => place(p),
        Operand::Move(p) => format!("move {}", place(p)),
        Operand::Const(c) => match c {
            Const::Unit => "()".to_owned(),
            Const::Int(v) => v.to_i128().to_string(),
            Const::Str(s) => format!("{s:?}"),
            Const::Bool(b) => b.to_string(),
            Const::Char(c) => format!("{c:?}"),
            Const::Item(loc) => format!("item {}", loc.display_name()),
            Const::Builtin(b) => format!("builtin {}", b.name()),
            Const::Fn(body) => format!("fn {}", body_name(*body)),
            Const::ExternFn { decl, .. } => format!("extern {}", decl.display_name()),
            Const::ConstBlock(body) => format!("const {}", body_name(*body)),
            Const::ConstParam(index) => format!("const param {index}"),
        },
    }
}

fn local(id: LocalId) -> String {
    format!("_{}", u32::from(id.into_raw()))
}

/// `_1` for a whole local, `_1.0.2` through a field-index projection (the
/// same dotted spelling `ValueExpr::Field` reads render with), `_1[_2]`
/// through an element projection, `_1.*` through a pointer deref.
fn place(p: &Place) -> String {
    let mut out = local(p.local);
    out.push_str(&projection_suffix(&p.projection));
    out
}

fn projection_suffix(projection: &[ProjElem]) -> String {
    let mut out = String::new();
    for elem in projection {
        match elem {
            ProjElem::Field(index) => {
                let _ = write!(out, ".{index}");
            }
            ProjElem::Index(op) => {
                let _ = write!(out, "[{}]", operand(op));
            }
            ProjElem::Deref => out.push_str(".*"),
        }
    }
    out
}

fn block(id: BlockId) -> String {
    format!("bb{}", u32::from(id.into_raw()))
}

fn body_name(id: BodyId) -> String {
    format!("b{}", u32::from(id.into_raw()))
}
