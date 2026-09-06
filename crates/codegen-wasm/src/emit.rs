//! MIR → WebAssembly: one function body per monomorphized instance.
//!
//! **Control flow.** MIR is an unstructured CFG; WebAssembly is
//! structured. Rather than a relooper, this backend uses the
//! always-correct fallback: one `loop` whose body is a `br_table` dispatch
//! over a block counter, with every MIR basic block sitting between two
//! `end`s. Every MIR edge costs a store to the counter and a branch.
//! Correct, obvious, and slow — the first thing a pre-mono LIR (P08) would
//! replace, by structuring control flow before it ever reaches a backend.
//!
//! **Values.** Every value is a run of consecutive `i64` locals (see
//! `layout.rs`). Reads produce "a base local index plus a slot offset";
//! writes pop the value into scratch locals and copy it into place. No
//! linear memory is used for user data at all — the module's memory holds
//! string bytes and nothing else.
//!
//! **Traps.** A trap writes its reason's index into the exported
//! `trap_code` global and executes `unreachable`. The global survives the
//! trap, so a host can ask *which* trap fired and compare it against what
//! the interpreter would have said.

use base_db::Db;
use eval::Value;
use hir::body::BinOp;
use hir::{ExprId, IntKind, ItemLoc, Ty};
use mir::{
    BlockId, Const, LocalId, MirBody, Operand, Place, ProjElem, Rvalue, StatementKind,
    TerminatorKind,
};

use crate::layout::{self, EqSlot, Unsupported};
use crate::mono::{CallTarget, InstanceKey, Mono, Refusal};
use crate::wasm::{FuncBody, ValType};
use crate::{Strings, TrapKind, Traps};

/// Where a value's slots come from.
enum Src {
    /// A run of wasm locals starting at this index.
    Locals(u32),
    /// Compile-time slots.
    Consts(Vec<i64>),
}

/// The maximum array length this backend will generate a dynamic index
/// for: one `select` chain per element per slot, so a big array would
/// generate an enormous function. Refused honestly instead.
const MAX_DYNAMIC_INDEX_LEN: u32 = 1024;

pub struct Emitter<'a, 'db> {
    pub mono: &'a mut Mono<'db>,
    pub strings: &'a mut Strings,
    pub traps: &'a mut Traps,
    /// Function index of the imported `print`.
    pub print: u32,
    /// Function index of the generated `str_eq` helper.
    pub str_eq: u32,
    /// Global index of `trap_code`.
    pub trap_global: u32,
    /// Globals holding a runtime-computed panic message, as
    /// `(offset, length)` into the module's memory.
    pub panic_offset: u32,
    pub panic_len: u32,
    /// Function index per registered instance, by registration order.
    pub func_indices: &'a [u32],
}

/// Per-function emission state.
struct Ctx<'db> {
    key: InstanceKey,
    loc: ItemLoc,
    mir: &'db MirBody,
    locals: Vec<Ty>,
    /// First wasm local of each MIR local.
    base: Vec<u32>,
    slots: Vec<u32>,
    body: FuncBody,
    pc: u32,
    block_count: u32,
    /// How deep the emitter currently is inside blocks opened *within*
    /// the current MIR block's code (each one shifts branch depths).
    depth: u32,
    /// Which MIR block is being emitted (fixes the depth of the dispatch
    /// loop from here).
    current: u32,
    /// Reusable `i64` scratch locals, and how many of them are claimed by
    /// the statement being emitted.
    scratch: Vec<u32>,
    scratch_used: u32,
}

impl<'db> Ctx<'db> {
    fn ty(&self, local: LocalId) -> Ty {
        self.locals[raw(local)].clone()
    }

    /// Branch depth of the dispatch `loop` from the current position.
    fn loop_depth(&self) -> u32 {
        (self.block_count - 1 - self.current) + self.depth
    }

    fn claim_scratch(&mut self, count: u32) -> u32 {
        if count == 0 {
            return 0;
        }
        while self.scratch.len() < (self.scratch_used + count) as usize {
            let index = self.body.local(ValType::I64);
            self.scratch.push(index);
        }
        let first = self.scratch[self.scratch_used as usize];
        self.scratch_used += count;
        // The pool is allocated in order, so a claim is contiguous.
        first
    }
}

impl<'a, 'db> Emitter<'a, 'db> {
    fn db(&self) -> &'db dyn Db {
        self.mono.db
    }

    /// The wasm signature of an instance: parameters are the value
    /// parameters' slots (dictionary parameters have none — they are gone),
    /// results are the return type's slots.
    pub fn signature(
        &mut self,
        key: &InstanceKey,
    ) -> Result<(Vec<ValType>, Vec<ValType>), Refusal> {
        let analysis = self.mono.analyze(key);
        let mir = &mir::mir_lowered(self.db(), key.func.value.item.to_id(self.db())).bodies
            [key.func.value.body];
        let entry_expr = mir.blocks[mir.entry].terminator.origin;
        let mut params = Vec::new();
        for param in &mir.params {
            let count = layout::slots(self.db(), &analysis.locals[raw(param)])
                .map_err(|err| Refusal::from_layout(err, &key.func.value.item, entry_expr))?;
            for _ in 0..count {
                params.push(ValType::I64);
            }
        }
        let count = layout::slots(self.db(), &analysis.locals[raw(mir.return_local())])
            .map_err(|err| Refusal::from_layout(err, &key.func.value.item, entry_expr))?;
        Ok((params, vec![ValType::I64; count as usize]))
    }

    pub fn emit(&mut self, key: &InstanceKey) -> Result<FuncBody, Refusal> {
        let analysis = self.mono.analyze(key);
        let loc = key.func.value.item.clone();
        let mir = &mir::mir_lowered(self.db(), loc.to_id(self.db())).bodies[key.func.value.body];
        let entry_expr = mir.blocks[mir.entry].terminator.origin;

        let count = mir.locals.len();
        let mut base = vec![0u32; count];
        let mut slots = vec![0u32; count];
        let mut next = 0;
        for param in &mir.params {
            let width = layout::slots(self.db(), &analysis.locals[raw(param)])
                .map_err(|err| Refusal::from_layout(err, &loc, entry_expr))?;
            base[raw(param)] = next;
            slots[raw(param)] = width;
            next += width;
        }
        let mut body = FuncBody::new(next);
        for (id, _) in mir.locals.iter() {
            if mir.params.contains(&id) {
                continue;
            }
            // Blame a local the backend cannot lay out where it is first
            // written — the `let` a reader would look at — rather than at
            // the top of the function.
            let width = layout::slots(self.db(), &analysis.locals[raw(id)]).map_err(|err| {
                Refusal::from_layout(err, &loc, local_origin(mir, id, entry_expr))
            })?;
            slots[raw(id)] = width;
            if width > 0 {
                let first = body.local(ValType::I64);
                for _ in 1..width {
                    body.local(ValType::I64);
                }
                base[raw(id)] = first;
            }
        }
        let pc = body.local(ValType::I32);

        let mut ctx = Ctx {
            key: key.clone(),
            loc,
            mir,
            locals: analysis.locals.clone(),
            base,
            slots,
            body,
            pc,
            block_count: mir.blocks.len() as u32,
            depth: 0,
            current: 0,
            scratch: Vec::new(),
            scratch_used: 0,
        };

        // pc = entry block
        ctx.body.i32_const(raw(mir.entry) as i32);
        ctx.body.local_set(ctx.pc);
        ctx.body.loop_void();
        for _ in 0..ctx.block_count {
            ctx.body.block_void();
        }
        // The innermost block holds only the dispatch.
        ctx.body.block_void();
        ctx.body.local_get(ctx.pc);
        // Inside the dispatch block, label 0 is the dispatch block
        // itself, so MIR block `i` is label `i + 1`.
        let targets: Vec<u32> = (0..ctx.block_count).map(|index| index + 1).collect();
        ctx.body.br_table(&targets, 1);
        ctx.body.end();
        ctx.body.unreachable();

        for (block_id, block) in mir.blocks.iter() {
            // Close this block's `block`, landing at its code.
            ctx.body.end();
            ctx.current = raw(block_id) as u32;
            ctx.depth = 0;
            for statement in &block.statements {
                ctx.scratch_used = 0;
                self.statement(&mut ctx, &statement.kind, statement.origin)?;
            }
            ctx.scratch_used = 0;
            self.terminator(
                &mut ctx,
                block_id,
                &block.terminator.kind,
                block.terminator.origin,
            )?;
        }
        // Close the dispatch loop. Falling out of it is unreachable: every
        // block's code ends in a branch, a return or a trap.
        ctx.body.end();
        ctx.body.unreachable();
        Ok(ctx.body)
    }

    // --- statements ------------------------------------------------------

    fn statement(
        &mut self,
        ctx: &mut Ctx<'db>,
        kind: &StatementKind,
        origin: ExprId,
    ) -> Result<(), Refusal> {
        let StatementKind::Assign { dest, rvalue } = kind;
        let dest_ty = self.place_ty(ctx, dest, origin)?;
        let width = self.slots(ctx, &dest_ty, origin)?;
        let pushed = self.rvalue(ctx, rvalue, &dest_ty, width, origin)?;
        // Deferred-error MIR can put a value of one shape where another
        // was expected (the program already has a diagnostic). Reconcile
        // rather than emit an ill-formed module.
        for _ in width..pushed {
            ctx.body.drop_();
        }
        for _ in pushed..width {
            ctx.body.i64_const(0);
        }
        // The value is on the stack; the destination's own index operands
        // are resolved after it — the interpreter's order.
        let scratch = ctx.claim_scratch(width);
        for slot in (0..width).rev() {
            ctx.body.local_set(scratch + slot);
        }
        self.store_place(ctx, dest, scratch, width, origin)
    }

    // --- rvalues ---------------------------------------------------------

    fn rvalue(
        &mut self,
        ctx: &mut Ctx<'db>,
        rvalue: &Rvalue,
        dest_ty: &Ty,
        width: u32,
        origin: ExprId,
    ) -> Result<u32, Refusal> {
        match rvalue {
            Rvalue::Use(op) => {
                let (src, ty) = self.operand(ctx, op, origin)?;
                let count = self.slots(ctx, &ty, origin)?;
                self.push_range(ctx, &src, 0, count, origin)?;
                Ok(count)
            }
            Rvalue::BinaryOp(op, l, r) => {
                self.binary(ctx, *op, l, r, origin)?;
                Ok(1)
            }
            Rvalue::UnaryNeg(op) => {
                self.negate(ctx, op, origin)?;
                Ok(1)
            }
            // Records, variant payloads and arrays are all the same thing
            // here: slots concatenate in the aggregate's canonical order,
            // which IS the order MIR hands the operands over in (records
            // sorted by name, payloads and elements positional). So the
            // `kind` needs no inspection at all.
            Rvalue::Aggregate { ops, .. } => {
                let mut total = 0;
                for op in ops {
                    let (src, ty) = self.operand(ctx, op, origin)?;
                    let count = self.slots(ctx, &ty, origin)?;
                    self.push_range(ctx, &src, 0, count, origin)?;
                    total += count;
                }
                Ok(total)
            }
            Rvalue::Field { base, index } => {
                let (src, ty) = self.operand(ctx, base, origin)?;
                let read = layout::field(self.db(), &ty, *index)
                    .map_err(|err| self.refuse(ctx, err, origin))?;
                let count = match read.ty() {
                    // Records, named structs and variant payloads: the
                    // element's own type says how wide the read is.
                    Some(field_ty) => self.slots(ctx, field_ty, origin)?,
                    // A TAGGED enum's payload column. Which variant this
                    // arm matched is not in MIR, so the element's type is
                    // unknowable here — but the DESTINATION's type is
                    // exactly that element's type (it is the binding the
                    // arm introduced), and a payload element always starts
                    // at its column offset. So the destination sizes the
                    // read: sizing it from the first variant that
                    // *declares* the position would under-read whenever a
                    // later variant is wider — see
                    // `a_tagged_enum_payload_read_is_sized_by_its_binding_not_by_variant_order`
                    // in tests/differential.rs.
                    None => width,
                };
                self.push_range(ctx, &src, read.offset(), count, origin)?;
                Ok(count)
            }
            Rvalue::Index { base, index } => {
                let (src, ty) = self.operand(ctx, base, origin)?;
                let (_, element_width, len) =
                    layout::element(self.db(), &ty).map_err(|err| self.refuse(ctx, err, origin))?;
                let element = self.index_into(ctx, &src, index, element_width, len, 0, origin)?;
                self.push_range(ctx, &Src::Locals(element), 0, element_width, origin)?;
                Ok(element_width)
            }
            // `[e; N]`: N copies. The count is a compile-time value by
            // checking, so it is materialized here, not looped over.
            Rvalue::Repeat { elem, count } => {
                let times = match self.const_int(ctx, count)? {
                    Some(value) => value,
                    None => {
                        return Err(Refusal::new(
                            "an array repeat count that is not a compile-time value",
                            &ctx.loc,
                            origin,
                        ));
                    }
                };
                let (src, ty) = self.operand(ctx, elem, origin)?;
                let each = self.slots(ctx, &ty, origin)?;
                for _ in 0..times {
                    self.push_range(ctx, &src, 0, each, origin)?;
                }
                Ok(each * times as u32)
            }
            // A function value: zero slots. Monomorphization already
            // recorded which function this is.
            Rvalue::Instantiate { .. } => Ok(0),
            Rvalue::WidenToEnum { op, index, .. } => {
                self.widen(ctx, op, *index, dest_ty, width, origin)
            }
            Rvalue::AddrOf { .. } | Rvalue::AddrOfStatic { .. } => Err(Refusal::new(
                "taking an address (`.&raw`) — raw pointers and the heap are \
                 out of scope for this backend",
                &ctx.loc,
                origin,
            )),
            // Refused by its own name, not folded into the raw arm above.
            // A borrow lowers to the same machine word, so this backend
            // COULD emit something that runs — and would silently drop the
            // exclusivity contract while doing it. Refusing is the whole
            // job here.
            Rvalue::Borrow { .. } => Err(Refusal::new(
                "a safe borrow (`.&` / `.&mut`)",
                &ctx.loc,
                origin,
            )),
        }
    }

    /// The variant → enum widening: inject the tag and place the payload
    /// into the enum's columns, zero-filling everything the variant does
    /// not occupy. The one place a tag ever comes into existence — and a
    /// real conversion, not a no-op: it costs a tag store plus the padding.
    #[allow(clippy::too_many_arguments)]
    fn widen(
        &mut self,
        ctx: &mut Ctx<'db>,
        op: &Operand,
        index: u32,
        dest_ty: &Ty,
        width: u32,
        origin: ExprId,
    ) -> Result<u32, Refusal> {
        // Everything is read off the DESTINATION type (X10). MIR types the
        // pre-conversion temp with the *post*-conversion type where the
        // widening happened at a direct check site, so the operand's own
        // type cannot be trusted to describe the payload it holds — but
        // the payload always starts at slot 0 of it, which is all this
        // needs.
        let enum_layout =
            layout::enum_layout(self.db(), dest_ty).map_err(|err| self.refuse(ctx, err, origin))?;
        let Some((_, payload)) = enum_layout.variants.get(index as usize).cloned() else {
            return Err(Refusal::new(
                format!("widening into `{}`", dest_ty.display()),
                &ctx.loc,
                origin,
            ));
        };
        let (src, _) = self.operand(ctx, op, origin)?;
        ctx.body.i64_const(i64::from(index));
        let mut emitted = 1;
        let mut offset = 0;
        for (position, column) in enum_layout.columns.iter().enumerate() {
            let column_width = enum_layout
                .columns
                .get(position + 1)
                .copied()
                .unwrap_or(enum_layout.slots)
                - column;
            let mut written = 0;
            if let Some(element_ty) = payload.get(position) {
                written = self.slots(ctx, element_ty, origin)?;
                self.push_range(ctx, &src, offset, written, origin)?;
                offset += written;
            }
            for _ in written..column_width {
                ctx.body.i64_const(0);
            }
            emitted += column_width;
        }
        debug_assert_eq!(
            emitted, width,
            "a widened value must fill exactly its enum's layout"
        );
        Ok(emitted)
    }

    // --- operands and places ---------------------------------------------

    /// Where an operand's slots live, and its concrete type.
    fn operand(
        &mut self,
        ctx: &mut Ctx<'db>,
        op: &Operand,
        origin: ExprId,
    ) -> Result<(Src, Ty), Refusal> {
        let ty = self
            .mono
            .operand_ty(&ctx.key, &ctx.locals, op)
            .map_err(|err| self.refuse(ctx, err, origin))?;
        match op {
            Operand::Copy(place) => {
                let base = self.read_place(ctx, place, origin)?;
                Ok((Src::Locals(base), ty))
            }
            Operand::Const(konst) => {
                let slots = self.const_slots(ctx, konst, &ty, origin)?;
                Ok((Src::Consts(slots), ty))
            }
        }
    }

    /// Resolve a place to the first wasm local of its value, emitting code
    /// for any dynamic index steps along the way.
    fn read_place(
        &mut self,
        ctx: &mut Ctx<'db>,
        place: &Place,
        origin: ExprId,
    ) -> Result<u32, Refusal> {
        let mut ty = ctx.ty(place.local);
        let mut base = ctx.base[raw(place.local)];
        for elem in &place.projection {
            match elem {
                ProjElem::Field(index) => {
                    let (offset, field) = self.field_of(ctx, &ty, *index, origin)?;
                    base += offset;
                    ty = field;
                }
                ProjElem::Index(index) => {
                    let (element_ty, width, len) = layout::element(self.db(), &ty)
                        .map_err(|err| self.refuse(ctx, err, origin))?;
                    let src = Src::Locals(base);
                    base = self.index_into(ctx, &src, index, width, len, 0, origin)?;
                    ty = element_ty;
                }
                ProjElem::Deref => {
                    return Err(Refusal::new(
                        "dereferencing a raw pointer (`p.*`) — raw pointers and the \
                         heap are out of scope for this backend",
                        &ctx.loc,
                        origin,
                    ));
                }
            }
        }
        Ok(base)
    }

    /// Read element `index` out of an array value, bounds-checked exactly
    /// where the interpreter checks it. Returns the first wasm local of
    /// the element.
    ///
    /// A compile-time index is a plain offset. A runtime index costs one
    /// `select` per element per slot — the price of keeping arrays in
    /// locals instead of memory.
    #[allow(clippy::too_many_arguments)]
    fn index_into(
        &mut self,
        ctx: &mut Ctx<'db>,
        src: &Src,
        index: &Operand,
        width: u32,
        len: u32,
        suffix: u32,
        origin: ExprId,
    ) -> Result<u32, Refusal> {
        if let Some(value) = self.const_int(ctx, index)? {
            if value >= u128::from(len) {
                // The checker squiggles this; MIR still lowers it, so the
                // compiled program traps with the same message.
                let trap = self.traps.intern(
                    TrapKind::IndexOutOfBounds,
                    hir::diag::index_out_of_bounds(u128::from(len), value),
                    true,
                );
                self.trap(ctx, trap);
                return Ok(match src {
                    Src::Locals(base) => *base,
                    Src::Consts(_) => ctx.claim_scratch(width),
                });
            }
            let offset = value as u32 * width + suffix;
            return match src {
                Src::Locals(base) => Ok(base + offset),
                Src::Consts(values) => {
                    let scratch = ctx.claim_scratch(width);
                    for slot in 0..width {
                        let value = values
                            .get((offset + slot) as usize)
                            .copied()
                            .unwrap_or_default();
                        ctx.body.i64_const(value);
                        ctx.body.local_set(scratch + slot);
                    }
                    Ok(scratch)
                }
            };
        }
        if len > MAX_DYNAMIC_INDEX_LEN {
            return Err(Refusal::new(
                format!(
                    "a runtime index into an array of {len} elements (this backend \
                     keeps arrays in registers, so it only generates dynamic \
                     indexing up to {MAX_DYNAMIC_INDEX_LEN} elements)"
                ),
                &ctx.loc,
                origin,
            ));
        }
        let (index_src, _) = self.operand(ctx, index, origin)?;
        let idx = ctx.claim_scratch(1);
        self.push_range(ctx, &index_src, 0, 1, origin)?;
        ctx.body.local_set(idx);
        // Bounds check: an out-of-range index is an ordinary trap.
        ctx.body.local_get(idx);
        ctx.body.i64_const(i64::from(len));
        ctx.body.i64_ge_u();
        ctx.body.if_void();
        ctx.depth += 1;
        let trap = self.traps.intern(
            TrapKind::IndexOutOfBounds,
            format!("index out of bounds: the length is {len} but the index is "),
            false,
        );
        self.trap(ctx, trap);
        ctx.depth -= 1;
        ctx.body.end();

        let out = ctx.claim_scratch(width);
        for slot in 0..width {
            self.push_range(ctx, src, suffix + slot, 1, origin)?;
            for element in 1..len {
                self.push_range(ctx, src, element * width + suffix + slot, 1, origin)?;
                ctx.body.local_get(idx);
                ctx.body.i64_const(i64::from(element));
                ctx.body.i64_ne();
                // `select` keeps the accumulator when the index does not
                // match this element.
                ctx.body.select();
            }
            ctx.body.local_set(out + slot);
        }
        Ok(out)
    }

    /// Copy `width` slots from scratch into a place.
    fn store_place(
        &mut self,
        ctx: &mut Ctx<'db>,
        place: &Place,
        value: u32,
        width: u32,
        origin: ExprId,
    ) -> Result<(), Refusal> {
        // Walk the static prefix; a dynamic index step switches to the
        // select-chain write below.
        let mut ty = ctx.ty(place.local);
        let mut base = ctx.base[raw(place.local)];
        for (position, elem) in place.projection.iter().enumerate() {
            match elem {
                ProjElem::Field(index) => {
                    let (offset, field) = self.field_of(ctx, &ty, *index, origin)?;
                    base += offset;
                    ty = field;
                }
                ProjElem::Index(index) => {
                    let (element_ty, element_width, len) = layout::element(self.db(), &ty)
                        .map_err(|err| self.refuse(ctx, err, origin))?;
                    if let Some(value) = self.const_int(ctx, index)? {
                        if value >= u128::from(len) {
                            let trap = self.traps.intern(
                                TrapKind::IndexOutOfBounds,
                                hir::diag::index_out_of_bounds(u128::from(len), value),
                                true,
                            );
                            self.trap(ctx, trap);
                            return Ok(());
                        }
                        base += value as u32 * element_width;
                        ty = element_ty;
                        continue;
                    }
                    // The suffix after the dynamic step must be static:
                    // one select chain is generated, not a nest of them.
                    let mut suffix = 0;
                    let mut suffix_ty = element_ty;
                    for elem in &place.projection[position + 1..] {
                        match elem {
                            ProjElem::Field(index) => {
                                let (offset, field) =
                                    self.field_of(ctx, &suffix_ty, *index, origin)?;
                                suffix += offset;
                                suffix_ty = field;
                            }
                            _ => {
                                return Err(Refusal::new(
                                    "an assignment through two runtime array indices",
                                    &ctx.loc,
                                    origin,
                                ));
                            }
                        }
                    }
                    return self.store_dynamic(
                        ctx,
                        base,
                        index,
                        element_width,
                        len,
                        suffix,
                        value,
                        width,
                        origin,
                    );
                }
                ProjElem::Deref => {
                    return Err(Refusal::new(
                        "writing through a raw pointer (`p.* = …`) — raw pointers and \
                         the heap are out of scope for this backend",
                        &ctx.loc,
                        origin,
                    ));
                }
            }
        }
        for slot in 0..width {
            ctx.body.local_get(value + slot);
            ctx.body.local_set(base + slot);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn store_dynamic(
        &mut self,
        ctx: &mut Ctx<'db>,
        base: u32,
        index: &Operand,
        element_width: u32,
        len: u32,
        suffix: u32,
        value: u32,
        width: u32,
        origin: ExprId,
    ) -> Result<(), Refusal> {
        if len > MAX_DYNAMIC_INDEX_LEN {
            return Err(Refusal::new(
                format!(
                    "a runtime index into an array of {len} elements (this backend \
                     keeps arrays in registers, so it only generates dynamic \
                     indexing up to {MAX_DYNAMIC_INDEX_LEN} elements)"
                ),
                &ctx.loc,
                origin,
            ));
        }
        let (index_src, _) = self.operand(ctx, index, origin)?;
        let idx = ctx.claim_scratch(1);
        self.push_range(ctx, &index_src, 0, 1, origin)?;
        ctx.body.local_set(idx);
        ctx.body.local_get(idx);
        ctx.body.i64_const(i64::from(len));
        ctx.body.i64_ge_u();
        ctx.body.if_void();
        ctx.depth += 1;
        let trap = self.traps.intern(
            TrapKind::IndexOutOfBounds,
            format!("index out of bounds: the length is {len} but the index is "),
            false,
        );
        self.trap(ctx, trap);
        ctx.depth -= 1;
        ctx.body.end();
        for element in 0..len {
            let target = base + element * element_width + suffix;
            for slot in 0..width {
                ctx.body.local_get(target + slot);
                ctx.body.local_get(value + slot);
                ctx.body.local_get(idx);
                ctx.body.i64_const(i64::from(element));
                // `select` keeps its FIRST operand when the condition
                // holds, so the condition is "this is not the element
                // being written".
                ctx.body.i64_ne();
                ctx.body.select();
                ctx.body.local_set(target + slot);
            }
        }
        Ok(())
    }

    fn push_range(
        &mut self,
        ctx: &mut Ctx<'db>,
        src: &Src,
        offset: u32,
        count: u32,
        origin: ExprId,
    ) -> Result<(), Refusal> {
        match src {
            Src::Locals(base) => {
                for slot in 0..count {
                    ctx.body.local_get(base + offset + slot);
                }
                Ok(())
            }
            Src::Consts(values) => {
                for slot in 0..count {
                    let Some(value) = values.get((offset + slot) as usize) else {
                        return Err(Refusal::new(
                            "a constant whose shape does not match its type",
                            &ctx.loc,
                            origin,
                        ));
                    };
                    ctx.body.i64_const(*value);
                }
                Ok(())
            }
        }
    }

    // --- constants -------------------------------------------------------

    /// The slots of a MIR constant. `static`s, `const { … }` blocks and
    /// const parameters are forced through the interpreter's const machine
    /// and baked in — there is no runtime initialization anywhere in the
    /// generated module.
    fn const_slots(
        &mut self,
        ctx: &mut Ctx<'db>,
        konst: &Const,
        ty: &Ty,
        origin: ExprId,
    ) -> Result<Vec<i64>, Refusal> {
        match konst {
            Const::Unit | Const::Fn(_) | Const::Builtin(_) => Ok(Vec::new()),
            Const::Int(value) => Ok(vec![value.to_i128() as i64]),
            Const::Bool(value) => Ok(vec![i64::from(*value)]),
            Const::Str(text) => {
                let (offset, len) = self.strings.intern(text);
                Ok(vec![i64::from(offset), i64::from(len)])
            }
            Const::Item(item) => {
                let value = self
                    .mono
                    .force_item(item)
                    .map_err(|message| Refusal::new(message, &ctx.loc, origin))?;
                self.value_slots(ctx, &value, ty, origin)
            }
            Const::ConstBlock(body) => {
                let value = self
                    .mono
                    .force_const_block(&ctx.loc, *body, ctx.key.func.value.const_args.clone())
                    .map_err(|message| Refusal::new(message, &ctx.loc, origin))?;
                self.value_slots(ctx, &value, ty, origin)
            }
            Const::ConstParam(index) => {
                let Some(value) = ctx.key.func.value.const_args.get(*index as usize).cloned()
                else {
                    return Err(Refusal::new(
                        "a const parameter with no value at this instantiation",
                        &ctx.loc,
                        origin,
                    ));
                };
                self.value_slots(ctx, &value, ty, origin)
            }
        }
    }

    /// Lay out an interpreter value — the bridge that makes const-baked
    /// statics literally the same values the interpreter computes.
    fn value_slots(
        &mut self,
        ctx: &mut Ctx<'db>,
        value: &Value,
        ty: &Ty,
        origin: ExprId,
    ) -> Result<Vec<i64>, Refusal> {
        let mismatch = |ctx: &Ctx<'db>| {
            Refusal::new(
                format!(
                    "a compile-time `{}` value the backend cannot lay out",
                    ty.display()
                ),
                &ctx.loc,
                origin,
            )
        };
        match value {
            Value::Unit | Value::Fn(_) | Value::Builtin(_) => Ok(Vec::new()),
            Value::Int(int) => Ok(vec![int.to_i128() as i64]),
            Value::Bool(b) => Ok(vec![i64::from(*b)]),
            Value::Str(text) => {
                let (offset, len) = self.strings.intern(text);
                Ok(vec![i64::from(offset), i64::from(len)])
            }
            Value::Record { fields } => {
                let mut out = Vec::new();
                for (index, (_, field)) in fields.iter().enumerate() {
                    let (_, field_ty) = self.field_of(ctx, ty, index as u32, origin)?;
                    out.extend(self.value_slots(ctx, field, &field_ty, origin)?);
                }
                Ok(out)
            }
            Value::Array(values) => {
                let (element_ty, _, _) =
                    layout::element(self.db(), ty).map_err(|err| self.refuse(ctx, err, origin))?;
                let mut out = Vec::new();
                for value in values {
                    out.extend(self.value_slots(ctx, value, &element_ty, origin)?);
                }
                Ok(out)
            }
            Value::Tuple(values) => {
                let mut out = Vec::new();
                for (index, value) in values.iter().enumerate() {
                    let (_, element_ty) = self.field_of(ctx, ty, index as u32, origin)?;
                    out.extend(self.value_slots(ctx, value, &element_ty, origin)?);
                }
                Ok(out)
            }
            Value::Variant { index, payload, .. } => {
                let enum_layout = layout::enum_layout(self.db(), ty)
                    .map_err(|err| self.refuse(ctx, err, origin))?;
                let Some((_, types)) = enum_layout.variants.get(*index as usize) else {
                    return Err(mismatch(ctx));
                };
                let mut out = vec![i64::from(*index)];
                for (position, column) in enum_layout.columns.iter().enumerate() {
                    let column_width = enum_layout
                        .columns
                        .get(position + 1)
                        .copied()
                        .unwrap_or(enum_layout.slots)
                        - column;
                    let mut written = 0;
                    if let (Some(value), Some(element_ty)) =
                        (payload.get(position), types.get(position))
                    {
                        let slots = self.value_slots(ctx, value, element_ty, origin)?;
                        written = slots.len() as u32;
                        out.extend(slots);
                    }
                    out.extend(std::iter::repeat_n(0, (column_width - written) as usize));
                }
                Ok(out)
            }
            Value::Ptr { .. } | Value::Uninit => Err(Refusal::new(
                "a compile-time pointer value — raw pointers and the heap are out \
                 of scope for this backend",
                &ctx.loc,
                origin,
            )),
        }
    }

    /// A compile-time integer operand, when it is one.
    fn const_int(&mut self, ctx: &mut Ctx<'db>, op: &Operand) -> Result<Option<u128>, Refusal> {
        let value = match op {
            Operand::Const(Const::Int(value)) => Some(*value),
            Operand::Const(Const::ConstBlock(body)) => {
                match self.mono.const_block(&ctx.key, *body) {
                    Some(Value::Int(value)) => Some(value),
                    _ => None,
                }
            }
            Operand::Const(Const::ConstParam(index)) => {
                match ctx.key.func.value.const_args.get(*index as usize) {
                    Some(Value::Int(value)) => Some(*value),
                    _ => None,
                }
            }
            Operand::Const(Const::Item(item)) => match self.mono.force_item(item) {
                Ok(Value::Int(value)) => Some(value),
                _ => None,
            },
            _ => None,
        };
        Ok(value.and_then(|value| u128::try_from(value.to_i128()).ok()))
    }

    // --- arithmetic ------------------------------------------------------

    /// Integer arithmetic with the ruled semantics: **overflow traps
    /// everywhere**, one behavior, no debug/release split. WebAssembly
    /// arithmetic wraps, so every operation carries its own check.
    fn binary(
        &mut self,
        ctx: &mut Ctx<'db>,
        op: BinOp,
        l: &Operand,
        r: &Operand,
        origin: ExprId,
    ) -> Result<(), Refusal> {
        if matches!(op, BinOp::Eq | BinOp::Ne) {
            return self.equality(ctx, op, l, r, origin);
        }
        let (lsrc, lty) = self.operand(ctx, l, origin)?;
        let (rsrc, _) = self.operand(ctx, r, origin)?;
        let Ty::Int(kind) = lty else {
            return Err(Refusal::new(
                format!("`{}` on `{}` values", op_symbol(op), lty.display()),
                &ctx.loc,
                origin,
            ));
        };
        let lhs = ctx.claim_scratch(1);
        let rhs = ctx.claim_scratch(1);
        self.push_range(ctx, &lsrc, 0, 1, origin)?;
        ctx.body.local_set(lhs);
        self.push_range(ctx, &rsrc, 0, 1, origin)?;
        ctx.body.local_set(rhs);
        match op {
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                ctx.body.local_get(lhs);
                ctx.body.local_get(rhs);
                match (op, kind.is_signed()) {
                    (BinOp::Lt, true) => ctx.body.i64_lt_s(),
                    (BinOp::Lt, false) => ctx.body.i64_lt_u(),
                    (BinOp::Le, true) => ctx.body.i64_le_s(),
                    (BinOp::Le, false) => ctx.body.i64_le_u(),
                    (BinOp::Gt, true) => ctx.body.i64_gt_s(),
                    (BinOp::Gt, false) => ctx.body.i64_gt_u(),
                    (BinOp::Ge, true) => ctx.body.i64_ge_s(),
                    (BinOp::Ge, false) => ctx.body.i64_ge_u(),
                    _ => unreachable!("comparison arm"),
                }
                ctx.body.i64_extend_i32_u();
                Ok(())
            }
            BinOp::Div => {
                let zero = self.traps.intern(
                    TrapKind::DivideByZero,
                    "attempt to divide by zero".to_owned(),
                    true,
                );
                ctx.body.local_get(rhs);
                ctx.body.i64_eqz();
                ctx.body.if_void();
                ctx.depth += 1;
                self.trap(ctx, zero);
                ctx.depth -= 1;
                ctx.body.end();
                if kind.is_signed() {
                    // `MIN / -1` overflows; wasm's own `div_s` would trap
                    // with no reason attached, so it is caught here first.
                    let overflow = self.overflow_trap(kind, "/");
                    ctx.body.local_get(lhs);
                    ctx.body.i64_const(min_of(kind));
                    ctx.body.i64_eq();
                    ctx.body.i64_extend_i32_u();
                    ctx.body.local_get(rhs);
                    ctx.body.i64_const(-1);
                    ctx.body.i64_eq();
                    ctx.body.i64_extend_i32_u();
                    ctx.body.i64_and();
                    ctx.body.i64_eqz();
                    ctx.body.i32_eqz();
                    ctx.body.if_void();
                    ctx.depth += 1;
                    self.trap(ctx, overflow);
                    ctx.depth -= 1;
                    ctx.body.end();
                }
                ctx.body.local_get(lhs);
                ctx.body.local_get(rhs);
                if kind.is_signed() {
                    ctx.body.i64_div_s();
                } else {
                    ctx.body.i64_div_u();
                }
                self.range_check(ctx, kind, "/");
                Ok(())
            }
            BinOp::Add | BinOp::Sub | BinOp::Mul => {
                let symbol = op_symbol(op);
                if width_of(kind) == 64 {
                    self.wide_arith(ctx, kind, op, lhs, rhs);
                } else {
                    ctx.body.local_get(lhs);
                    ctx.body.local_get(rhs);
                    match op {
                        BinOp::Add => ctx.body.i64_add(),
                        BinOp::Sub => ctx.body.i64_sub(),
                        _ => ctx.body.i64_mul(),
                    }
                    self.range_check(ctx, kind, symbol);
                }
                Ok(())
            }
            BinOp::Eq | BinOp::Ne => unreachable!("handled above"),
        }
    }

    /// The 64-bit cases, where no wider carrier is available: overflow is
    /// detected from the wrapped result itself.
    fn wide_arith(&mut self, ctx: &mut Ctx<'db>, kind: IntKind, op: BinOp, lhs: u32, rhs: u32) {
        let trap = self.overflow_trap(kind, op_symbol(op));
        let result = ctx.claim_scratch(1);
        ctx.body.local_get(lhs);
        ctx.body.local_get(rhs);
        match op {
            BinOp::Add => ctx.body.i64_add(),
            BinOp::Sub => ctx.body.i64_sub(),
            _ => ctx.body.i64_mul(),
        }
        ctx.body.local_set(result);
        // Push the "overflowed" condition as an i32.
        match (op, kind.is_signed()) {
            // Unsigned add: the sum wrapped iff it came out below an operand.
            (BinOp::Add, false) => {
                ctx.body.local_get(result);
                ctx.body.local_get(lhs);
                ctx.body.i64_lt_u();
            }
            (BinOp::Sub, false) => {
                ctx.body.local_get(lhs);
                ctx.body.local_get(rhs);
                ctx.body.i64_lt_u();
            }
            // Unsigned multiply: recover an operand by dividing back.
            (BinOp::Mul, false) => {
                ctx.body.local_get(lhs);
                ctx.body.i64_eqz();
                ctx.body.if_i64();
                ctx.body.i64_const(0);
                ctx.body.else_();
                ctx.body.local_get(result);
                ctx.body.local_get(lhs);
                ctx.body.i64_div_u();
                ctx.body.local_get(rhs);
                ctx.body.i64_ne();
                ctx.body.i64_extend_i32_u();
                ctx.body.end();
                ctx.body.i64_eqz();
                ctx.body.i32_eqz();
            }
            // Signed add: overflow iff both operands differ in sign from
            // the result.
            (BinOp::Add, true) => {
                ctx.body.local_get(lhs);
                ctx.body.local_get(result);
                ctx.body.i64_xor();
                ctx.body.local_get(rhs);
                ctx.body.local_get(result);
                ctx.body.i64_xor();
                ctx.body.i64_and();
                ctx.body.i64_const(0);
                ctx.body.i64_lt_s();
            }
            (BinOp::Sub, true) => {
                ctx.body.local_get(lhs);
                ctx.body.local_get(rhs);
                ctx.body.i64_xor();
                ctx.body.local_get(lhs);
                ctx.body.local_get(result);
                ctx.body.i64_xor();
                ctx.body.i64_and();
                ctx.body.i64_const(0);
                ctx.body.i64_lt_s();
            }
            (BinOp::Mul, true) => {
                // `lhs == 0` cannot overflow; `lhs == -1 && rhs == MIN`
                // must be caught before the division, which would itself
                // trap on `MIN / -1`.
                ctx.body.local_get(lhs);
                ctx.body.i64_eqz();
                ctx.body.if_i64();
                ctx.body.i64_const(0);
                ctx.body.else_();
                ctx.body.local_get(lhs);
                ctx.body.i64_const(-1);
                ctx.body.i64_eq();
                ctx.body.i64_extend_i32_u();
                ctx.body.local_get(rhs);
                ctx.body.i64_const(i64::MIN);
                ctx.body.i64_eq();
                ctx.body.i64_extend_i32_u();
                ctx.body.i64_and();
                ctx.body.i64_eqz();
                ctx.body.i32_eqz();
                ctx.body.if_i64();
                ctx.body.i64_const(1);
                ctx.body.else_();
                ctx.body.local_get(result);
                ctx.body.local_get(lhs);
                ctx.body.i64_div_s();
                ctx.body.local_get(rhs);
                ctx.body.i64_ne();
                ctx.body.i64_extend_i32_u();
                ctx.body.end();
                ctx.body.end();
                ctx.body.i64_eqz();
                ctx.body.i32_eqz();
            }
            _ => unreachable!("wide_arith takes add/sub/mul only"),
        }
        ctx.body.if_void();
        ctx.depth += 1;
        self.trap(ctx, trap);
        ctx.depth -= 1;
        ctx.body.end();
        ctx.body.local_get(result);
    }

    /// Range-check a result computed in the wide carrier against the
    /// narrow type it must fit — the shape every `i/u 8/16/32` operation
    /// takes.
    fn range_check(&mut self, ctx: &mut Ctx<'db>, kind: IntKind, symbol: &str) {
        if width_of(kind) == 64 {
            return;
        }
        let trap = self.overflow_trap(kind, symbol);
        let result = ctx.claim_scratch(1);
        ctx.body.local_set(result);
        ctx.body.local_get(result);
        ctx.body.i64_const(max_of(kind));
        ctx.body.i64_gt_s();
        ctx.body.local_get(result);
        ctx.body.i64_const(min_of(kind));
        ctx.body.i64_lt_s();
        ctx.body.i32_add();
        ctx.body.if_void();
        ctx.depth += 1;
        self.trap(ctx, trap);
        ctx.depth -= 1;
        ctx.body.end();
        ctx.body.local_get(result);
    }

    /// The interpreter's overflow message names the operand VALUES
    /// (`\`255 + 1\` does not fit in \`u8\``), which only exist at
    /// runtime — so the trap entry carries the prefix a compiled trap can
    /// promise, and the operation it came from as detail.
    fn overflow_trap(&mut self, kind: IntKind, symbol: &str) -> u32 {
        self.traps.intern_detailed(
            TrapKind::Overflow,
            "arithmetic overflow: ".to_owned(),
            false,
            format!("`{symbol}` on `{}`", kind.name()),
        )
    }

    /// `-x`: negate in the wide carrier and range-check — on an unsigned
    /// type every non-zero operand traps, and so does the signed minimum.
    fn negate(&mut self, ctx: &mut Ctx<'db>, op: &Operand, origin: ExprId) -> Result<(), Refusal> {
        let (src, ty) = self.operand(ctx, op, origin)?;
        let Ty::Int(kind) = ty else {
            return Err(Refusal::new(
                format!("negating a `{}` value", ty.display()),
                &ctx.loc,
                origin,
            ));
        };
        let value = ctx.claim_scratch(1);
        self.push_range(ctx, &src, 0, 1, origin)?;
        ctx.body.local_set(value);
        let trap = self.overflow_trap(kind, "-");
        if !kind.is_signed() {
            ctx.body.local_get(value);
            ctx.body.i64_eqz();
            ctx.body.i32_eqz();
            ctx.body.if_void();
            ctx.depth += 1;
            self.trap(ctx, trap);
            ctx.depth -= 1;
            ctx.body.end();
            ctx.body.i64_const(0);
            return Ok(());
        }
        ctx.body.local_get(value);
        ctx.body.i64_const(min_of(kind));
        ctx.body.i64_eq();
        ctx.body.if_void();
        ctx.depth += 1;
        self.trap(ctx, trap);
        ctx.depth -= 1;
        ctx.body.end();
        ctx.body.i64_const(0);
        ctx.body.local_get(value);
        ctx.body.i64_sub();
        Ok(())
    }

    /// Structural equality, slot by slot, with `str` slots compared by
    /// their bytes.
    fn equality(
        &mut self,
        ctx: &mut Ctx<'db>,
        op: BinOp,
        l: &Operand,
        r: &Operand,
        origin: ExprId,
    ) -> Result<(), Refusal> {
        let (lsrc, lty) = self.operand(ctx, l, origin)?;
        let (rsrc, rty) = self.operand(ctx, r, origin)?;
        // A variant-typed operand compared against its enum cannot happen
        // (widening is explicit in MIR), so one plan serves both sides.
        let plan = layout::eq_plan(self.db(), &lty)
            .or_else(|_| layout::eq_plan(self.db(), &rty))
            .map_err(|err| self.refuse(ctx, err, origin))?;
        ctx.body.i64_const(1);
        for (slot, entry) in plan.iter().enumerate() {
            let slot = slot as u32;
            match entry {
                EqSlot::StrLen => {}
                EqSlot::Value => {
                    self.push_range(ctx, &lsrc, slot, 1, origin)?;
                    self.push_range(ctx, &rsrc, slot, 1, origin)?;
                    ctx.body.i64_eq();
                    ctx.body.i64_extend_i32_u();
                    ctx.body.i64_and();
                }
                EqSlot::StrStart => {
                    self.push_range(ctx, &lsrc, slot, 2, origin)?;
                    self.push_range(ctx, &rsrc, slot, 2, origin)?;
                    ctx.body.call(self.str_eq);
                    ctx.body.i64_and();
                }
            }
        }
        if matches!(op, BinOp::Ne) {
            ctx.body.i64_eqz();
            ctx.body.i64_extend_i32_u();
        }
        Ok(())
    }

    // --- terminators -----------------------------------------------------

    fn terminator(
        &mut self,
        ctx: &mut Ctx<'db>,
        block: BlockId,
        kind: &TerminatorKind,
        origin: ExprId,
    ) -> Result<(), Refusal> {
        match kind {
            TerminatorKind::Goto { target } => {
                self.goto(ctx, *target);
                Ok(())
            }
            TerminatorKind::SwitchBool {
                discr,
                then_block,
                else_block,
            } => {
                let (src, _) = self.operand(ctx, discr, origin)?;
                ctx.body.i32_const(raw(then_block) as i32);
                ctx.body.i32_const(raw(else_block) as i32);
                self.push_range(ctx, &src, 0, 1, origin)?;
                ctx.body.i32_wrap_i64();
                ctx.body.select();
                ctx.body.local_set(ctx.pc);
                ctx.body.br(ctx.loop_depth());
                Ok(())
            }
            TerminatorKind::SwitchVariant {
                discr,
                arms,
                otherwise,
                ..
            } => {
                let (src, _) = self.operand(ctx, discr, origin)?;
                let tag = ctx.claim_scratch(1);
                // Slot 0 of a tagged value is its variant index.
                self.push_range(ctx, &src, 0, 1, origin)?;
                ctx.body.local_set(tag);
                for (index, target) in arms {
                    ctx.body.local_get(tag);
                    ctx.body.i64_const(i64::from(*index));
                    ctx.body.i64_eq();
                    ctx.body.if_void();
                    ctx.depth += 1;
                    self.goto(ctx, *target);
                    ctx.depth -= 1;
                    ctx.body.end();
                }
                self.goto(ctx, *otherwise);
                Ok(())
            }
            TerminatorKind::Call {
                args, dest, target, ..
            } => {
                let analysis = self.mono.analyze(&ctx.key);
                let call = analysis.calls.get(&block).cloned();
                match call {
                    Some(CallTarget::Print) => {
                        let Some(op) = args.first() else {
                            return Err(Refusal::new(
                                "a `print` with no argument",
                                &ctx.loc,
                                origin,
                            ));
                        };
                        let (src, _) = self.operand(ctx, op, origin)?;
                        self.push_range(ctx, &src, 0, 1, origin)?;
                        ctx.body.i32_wrap_i64();
                        self.push_range(ctx, &src, 1, 1, origin)?;
                        ctx.body.i32_wrap_i64();
                        ctx.body.call(self.print);
                    }
                    Some(CallTarget::Panic) => {
                        // A literal message can be baked into the trap
                        // table; anything else is only known at runtime,
                        // so the module publishes it the way it publishes
                        // its output — as (offset, length) into its own
                        // memory, in two exported globals a host reads
                        // after the trap.
                        match args.first() {
                            Some(Operand::Const(Const::Str(text))) => {
                                let trap = self.traps.intern(TrapKind::Panic, text.clone(), true);
                                self.trap(ctx, trap);
                            }
                            Some(op) => {
                                let (src, _) = self.operand(ctx, op, origin)?;
                                self.push_range(ctx, &src, 0, 1, origin)?;
                                ctx.body.global_set(self.panic_offset);
                                self.push_range(ctx, &src, 1, 1, origin)?;
                                ctx.body.global_set(self.panic_len);
                                let trap = self.traps.intern_detailed(
                                    TrapKind::Panic,
                                    String::new(),
                                    false,
                                    "the message is computed at runtime; read it from \
                                     the `panic_message` globals"
                                        .to_owned(),
                                );
                                self.trap(ctx, trap);
                            }
                            None => {
                                return Err(Refusal::new(
                                    "a `panic` with no argument",
                                    &ctx.loc,
                                    origin,
                                ));
                            }
                        }
                        // `panic` diverges: no continuation is emitted.
                        return Ok(());
                    }
                    Some(CallTarget::Instance(callee)) => {
                        let mut pushed = 0;
                        for op in args {
                            let (src, ty) = self.operand(ctx, op, origin)?;
                            let width = self.slots(ctx, &ty, origin)?;
                            self.push_range(ctx, &src, 0, width, origin)?;
                            pushed += width;
                        }
                        // Defense in depth. Under the dispatch-loop shape
                        // stray operands are LEGALLY abandoned at every
                        // block exit, so a module whose call arity is
                        // wrong still validates and quietly computes with
                        // the wrong values — a wasm engine's own
                        // validation cannot catch it (a type argument
                        // mis-derived as the dummy `()`, for instance,
                        // would otherwise pass silently; see
                        // `a_fn_literal_passed_to_a_generic_keeps_its_own_return_type`
                        // in tests/differential.rs). An argument list that
                        // does not fill the callee's parameters is a bug
                        // in this backend, and it stops here instead of
                        // shipping.
                        let expected = self.signature(&callee)?.0.len() as u32;
                        if pushed != expected {
                            return Err(Refusal::new(
                                format!(
                                    "this call (the backend built {pushed} argument slot(s) \
                                     for a function taking {expected} — that is a bug in the \
                                     wasm backend, not in your program)"
                                ),
                                &ctx.loc,
                                origin,
                            ));
                        }
                        let Some(slot) = self.mono.func_index(&callee) else {
                            return Err(Refusal::new(
                                "a call to a function that was never registered",
                                &ctx.loc,
                                origin,
                            ));
                        };
                        ctx.body.call(self.func_indices[slot as usize]);
                        // The callee's result count and the destination's
                        // width can genuinely differ: inference recovers a
                        // mismatch by trusting an annotation, so a call to
                        // a `!`-returning function can be *typed* by the
                        // expectation at its use site. Reconcile — the
                        // surplus code is dead either way, since such a
                        // call never returns.
                        let results = self.signature(&callee)?.1.len() as u32;
                        let width = ctx.slots[raw(dest)];
                        let base = ctx.base[raw(dest)];
                        for _ in width..results {
                            ctx.body.drop_();
                        }
                        for _ in results..width {
                            ctx.body.i64_const(0);
                        }
                        for slot in (0..width).rev() {
                            ctx.body.local_set(base + slot);
                        }
                    }
                    Some(CallTarget::Refused(refusal)) => return Err(refusal),
                    None => {
                        return Err(Refusal::new(
                            "a call the backend did not resolve",
                            &ctx.loc,
                            origin,
                        ));
                    }
                }
                match target {
                    Some(target) => self.goto(ctx, *target),
                    // The callee's type says it never returns.
                    None => ctx.body.unreachable(),
                }
                Ok(())
            }
            TerminatorKind::Return => {
                let ret = ctx.mir.return_local();
                let width = ctx.slots[raw(ret)];
                let base = ctx.base[raw(ret)];
                for slot in 0..width {
                    ctx.body.local_get(base + slot);
                }
                ctx.body.return_();
                Ok(())
            }
            // A planted diagnostic: the compiled program aborts with
            // exactly the message the editor shows and the interpreter
            // prints.
            TerminatorKind::Trap { message, .. } => {
                let trap = self
                    .traps
                    .intern(TrapKind::Diagnostic, message.clone(), true);
                self.trap(ctx, trap);
                Ok(())
            }
            // A const-context violation. Compiled code is not a const
            // context (the runner's `const_depth` 0 escape), so it falls
            // through exactly as it does under the interpreter.
            TerminatorKind::ConstTrap { target, .. } => {
                self.goto(ctx, *target);
                Ok(())
            }
            TerminatorKind::Unreachable => {
                let trap = self.traps.intern(
                    TrapKind::Internal,
                    "entered an unreachable block".to_owned(),
                    true,
                );
                self.trap(ctx, trap);
                Ok(())
            }
        }
    }

    fn goto(&mut self, ctx: &mut Ctx<'db>, target: BlockId) {
        ctx.body.i32_const(raw(target) as i32);
        ctx.body.local_set(ctx.pc);
        ctx.body.br(ctx.loop_depth());
    }

    /// Record which trap fired, then stop.
    fn trap(&mut self, ctx: &mut Ctx<'db>, index: u32) {
        ctx.body.i32_const(index as i32);
        ctx.body.global_set(self.trap_global);
        ctx.body.unreachable();
    }

    // --- helpers ---------------------------------------------------------

    fn slots(&mut self, ctx: &Ctx<'db>, ty: &Ty, origin: ExprId) -> Result<u32, Refusal> {
        layout::slots(self.db(), ty).map_err(|err| Refusal::from_layout(err, &ctx.loc, origin))
    }

    fn refuse(&self, ctx: &Ctx<'db>, err: Unsupported, origin: ExprId) -> Refusal {
        Refusal::from_layout(err, &ctx.loc, origin)
    }

    /// A field read that MUST know the element's type — every caller that
    /// continues walking into the element, or lays a compile-time value
    /// out through it. A tagged enum's payload column has no knowable
    /// type ([`layout::FieldRead`]), so this refuses rather than guesses.
    /// MIR does not emit such a read today (place projections come from
    /// NAMED field access, and enums have no named fields; payload reads
    /// are `Rvalue::Field`, which is destination-sized above) — this is
    /// the guard that keeps it that way.
    fn field_of(
        &self,
        ctx: &Ctx<'db>,
        ty: &Ty,
        index: u32,
        origin: ExprId,
    ) -> Result<(u32, Ty), Refusal> {
        let read =
            layout::field(self.db(), ty, index).map_err(|err| self.refuse(ctx, err, origin))?;
        match read.ty() {
            Some(field) => Ok((read.offset(), field.clone())),
            None => Err(Refusal::new(
                format!(
                    "projecting into a payload of the tagged enum `{}` (which variant \
                     it holds is not recorded on the read, so its type is not knowable \
                     here)",
                    ty.display()
                ),
                &ctx.loc,
                origin,
            )),
        }
    }

    fn place_ty(
        &mut self,
        ctx: &mut Ctx<'db>,
        place: &Place,
        origin: ExprId,
    ) -> Result<Ty, Refusal> {
        let mut ty = ctx.ty(place.local);
        for elem in &place.projection {
            ty = match elem {
                ProjElem::Field(index) => self.field_of(ctx, &ty, *index, origin)?.1,
                ProjElem::Index(_) => {
                    layout::element(self.db(), &ty)
                        .map_err(|err| self.refuse(ctx, err, origin))?
                        .0
                }
                ProjElem::Deref => {
                    return Err(Refusal::new(
                        "writing through a raw pointer (`p.* = …`) — raw pointers and \
                         the heap are out of scope for this backend",
                        &ctx.loc,
                        origin,
                    ));
                }
            };
        }
        Ok(ty)
    }
}

/// The `str_eq` support function: byte-wise comparison of two
/// `(offset, length)` pairs, returning 1 or 0 in an `i64`.
///
/// This is the module's only piece of generated support code, and it is
/// generated INTO the module — there is no runtime library to link.
pub fn str_eq_body() -> FuncBody {
    let mut body = FuncBody::new(4);
    // Parameters: the two `(offset, length)` pairs.
    let (a_off, a_len, b_off, b_len) = (0, 1, 2, 3);
    let index = body.local(ValType::I64);
    // Different lengths cannot be equal.
    body.local_get(a_len);
    body.local_get(b_len);
    body.i64_ne();
    body.if_void();
    body.i64_const(0);
    body.return_();
    body.end();
    body.i64_const(0);
    body.local_set(index);
    body.loop_void();
    // Done: every byte matched.
    body.local_get(index);
    body.local_get(a_len);
    body.i64_ge_u();
    body.if_void();
    body.i64_const(1);
    body.return_();
    body.end();
    body.local_get(a_off);
    body.local_get(index);
    body.i64_add();
    body.i32_wrap_i64();
    body.i32_load8_u();
    body.local_get(b_off);
    body.local_get(index);
    body.i64_add();
    body.i32_wrap_i64();
    body.i32_load8_u();
    body.i32_ne();
    body.if_void();
    body.i64_const(0);
    body.return_();
    body.end();
    body.local_get(index);
    body.i64_const(1);
    body.i64_add();
    body.local_set(index);
    body.br(0);
    body.end();
    // The loop only leaves through a `return`; this keeps the function
    // well-formed for the validator.
    body.unreachable();
    body
}

fn op_symbol(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
    }
}

fn width_of(kind: IntKind) -> u32 {
    match kind {
        IntKind::I8 | IntKind::U8 => 8,
        IntKind::I16 | IntKind::U16 => 16,
        IntKind::I32 | IntKind::U32 => 32,
        // The usize fork (P07): this backend pins `usize`/`isize` to 64
        // bits, matching the interpreter's platform — sound only while
        // the compiled subset has no pointers.
        IntKind::I64 | IntKind::U64 | IntKind::Usize | IntKind::Isize => 64,
    }
}

fn min_of(kind: IntKind) -> i64 {
    if !kind.is_signed() {
        return 0;
    }
    match width_of(kind) {
        8 => i64::from(i8::MIN),
        16 => i64::from(i16::MIN),
        32 => i64::from(i32::MIN),
        _ => i64::MIN,
    }
}

fn max_of(kind: IntKind) -> i64 {
    match (width_of(kind), kind.is_signed()) {
        (8, true) => i64::from(i8::MAX),
        (8, false) => i64::from(u8::MAX),
        (16, true) => i64::from(i16::MAX),
        (16, false) => i64::from(u16::MAX),
        (32, true) => i64::from(i32::MAX),
        (32, false) => i64::from(u32::MAX),
        (_, true) => i64::MAX,
        (_, false) => -1, // u64::MAX as a bit pattern; unused (64-bit
                          // types take the wide-arithmetic path, never a range check).
    }
}

/// Where a local is first assigned — the honest place to blame it.
fn local_origin(mir: &MirBody, local: LocalId, fallback: ExprId) -> ExprId {
    for (_, block) in mir.blocks.iter() {
        for statement in &block.statements {
            let StatementKind::Assign { dest, .. } = &statement.kind;
            if dest.local == local {
                return statement.origin;
            }
        }
    }
    fallback
}

fn raw<T>(idx: impl std::borrow::Borrow<la_arena::Idx<T>>) -> usize {
    u32::from(idx.borrow().into_raw()) as usize
}
