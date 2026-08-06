//! A hand-rolled WebAssembly binary encoder — everything this backend
//! needs to write a module, and nothing else.
//!
//! WHY HAND-ROLLED (flagged decision): the binary format is small, frozen
//! (the 1.0 core plus multi-value, both a decade stable) and fully
//! specified; the shapes we emit are few. A writer dependency would buy
//! convenience at the cost of the project's locked-stack instinct, so the
//! compiler crate takes NO new runtime dependency at all. The safety net
//! is not our own confidence: every module the differential harness runs
//! is *validated* by the engine before execution, so a malformed section
//! or a stack-typed mistake fails a test rather than shipping.
//!
//! Only the subset used by [`crate::emit`] is implemented. Instruction
//! opcodes are written as named constructors so the emitter never handles
//! raw bytes.

/// A WebAssembly value type. This backend uses exactly two: `i64` for
/// every Must value slot (see `layout.rs` for why storage is uniform), and
/// `i32` for module-internal plumbing (the block dispatch counter, memory
/// offsets at the `print` boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValType {
    I32,
    I64,
}

impl ValType {
    fn code(self) -> u8 {
        match self {
            ValType::I32 => 0x7F,
            ValType::I64 => 0x7E,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FuncType {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

/// What an export names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportKind {
    Func,
    Memory,
    Global,
}

impl ExportKind {
    fn code(self) -> u8 {
        match self {
            ExportKind::Func => 0x00,
            ExportKind::Memory => 0x02,
            ExportKind::Global => 0x03,
        }
    }
}

/// The module under construction. Function indices are assigned in the
/// order functions are added, imports first — the WebAssembly index space
/// rule, made explicit here so the emitter can hand out indices before
/// bodies exist.
#[derive(Debug, Default)]
pub struct Module {
    types: Vec<FuncType>,
    imports: Vec<(String, String, u32)>,
    /// Type index per *defined* (non-imported) function.
    funcs: Vec<u32>,
    /// Encoded body per defined function, parallel to `funcs`.
    bodies: Vec<Vec<u8>>,
    exports: Vec<(String, ExportKind, u32)>,
    globals: Vec<(ValType, bool, i64)>,
    memory_pages: u32,
    data: Vec<(u32, Vec<u8>)>,
    /// Debug names for the `name` custom section, by function index.
    func_names: Vec<(u32, String)>,
    custom: Vec<(String, Vec<u8>)>,
}

impl Module {
    pub fn new() -> Module {
        Module::default()
    }

    /// Intern a function type, returning its index.
    pub fn func_type(&mut self, params: Vec<ValType>, results: Vec<ValType>) -> u32 {
        let ty = FuncType { params, results };
        if let Some(index) = self.types.iter().position(|existing| *existing == ty) {
            return index as u32;
        }
        self.types.push(ty);
        (self.types.len() - 1) as u32
    }

    /// Declare an imported function. Must be called before any defined
    /// function is added: imports occupy the low function indices.
    pub fn import_func(&mut self, module: &str, name: &str, type_index: u32) -> u32 {
        assert!(
            self.funcs.is_empty(),
            "imports must be declared before defined functions"
        );
        self.imports
            .push((module.to_owned(), name.to_owned(), type_index));
        (self.imports.len() - 1) as u32
    }

    /// Whether `(module, field)` is already imported.
    ///
    /// Asked of the module rather than of a hand-written list, so the answer
    /// cannot drift from the imports actually declared: a name becomes
    /// reserved by being used, not by being written down twice.
    pub fn imports_func(&self, module: &str, field: &str) -> bool {
        self.imports
            .iter()
            .any(|(m, f, _)| m == module && f == field)
    }

    /// Reserve a function index for a body supplied later by
    /// [`Module::set_body`] — the two-phase shape monomorphization needs
    /// (call sites must know callee indices before every body is built).
    pub fn declare_func(&mut self, type_index: u32, name: String) -> u32 {
        self.funcs.push(type_index);
        self.bodies.push(Vec::new());
        let index = (self.imports.len() + self.funcs.len() - 1) as u32;
        self.func_names.push((index, name));
        index
    }

    pub fn set_body(&mut self, func_index: u32, body: FuncBody) {
        let slot = func_index as usize - self.imports.len();
        self.bodies[slot] = body.finish();
    }

    pub fn export(&mut self, name: &str, kind: ExportKind, index: u32) {
        self.exports.push((name.to_owned(), kind, index));
    }

    /// Declare the module's single memory, sized in 64KiB pages.
    pub fn memory(&mut self, pages: u32) {
        self.memory_pages = pages;
    }

    pub fn global(&mut self, ty: ValType, mutable: bool, init: i64) -> u32 {
        self.globals.push((ty, mutable, init));
        (self.globals.len() - 1) as u32
    }

    pub fn data_segment(&mut self, offset: u32, bytes: Vec<u8>) {
        self.data.push((offset, bytes));
    }

    pub fn custom_section(&mut self, name: &str, payload: Vec<u8>) {
        self.custom.push((name.to_owned(), payload));
    }

    /// Encode the whole module.
    pub fn finish(self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"\0asm");
        out.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);

        // 1: types
        if !self.types.is_empty() {
            let mut section = Vec::new();
            uleb(&mut section, self.types.len() as u64);
            for ty in &self.types {
                section.push(0x60);
                uleb(&mut section, ty.params.len() as u64);
                for param in &ty.params {
                    section.push(param.code());
                }
                uleb(&mut section, ty.results.len() as u64);
                for result in &ty.results {
                    section.push(result.code());
                }
            }
            emit_section(&mut out, 1, &section);
        }

        // 2: imports
        if !self.imports.is_empty() {
            let mut section = Vec::new();
            uleb(&mut section, self.imports.len() as u64);
            for (module, name, type_index) in &self.imports {
                name_bytes(&mut section, module);
                name_bytes(&mut section, name);
                section.push(0x00); // func
                uleb(&mut section, u64::from(*type_index));
            }
            emit_section(&mut out, 2, &section);
        }

        // 3: functions
        if !self.funcs.is_empty() {
            let mut section = Vec::new();
            uleb(&mut section, self.funcs.len() as u64);
            for type_index in &self.funcs {
                uleb(&mut section, u64::from(*type_index));
            }
            emit_section(&mut out, 3, &section);
        }

        // 5: memory
        if self.memory_pages > 0 {
            let mut section = Vec::new();
            uleb(&mut section, 1);
            section.push(0x00); // limits: min only
            uleb(&mut section, u64::from(self.memory_pages));
            emit_section(&mut out, 5, &section);
        }

        // 6: globals
        if !self.globals.is_empty() {
            let mut section = Vec::new();
            uleb(&mut section, self.globals.len() as u64);
            for (ty, mutable, init) in &self.globals {
                section.push(ty.code());
                section.push(u8::from(*mutable));
                match ty {
                    ValType::I32 => {
                        section.push(0x41);
                        sleb(&mut section, *init);
                    }
                    ValType::I64 => {
                        section.push(0x42);
                        sleb(&mut section, *init);
                    }
                }
                section.push(0x0B); // end
            }
            emit_section(&mut out, 6, &section);
        }

        // 7: exports
        if !self.exports.is_empty() {
            let mut section = Vec::new();
            uleb(&mut section, self.exports.len() as u64);
            for (name, kind, index) in &self.exports {
                name_bytes(&mut section, name);
                section.push(kind.code());
                uleb(&mut section, u64::from(*index));
            }
            emit_section(&mut out, 7, &section);
        }

        // 10: code
        if !self.bodies.is_empty() {
            let mut section = Vec::new();
            uleb(&mut section, self.bodies.len() as u64);
            for body in &self.bodies {
                uleb(&mut section, body.len() as u64);
                section.extend_from_slice(body);
            }
            emit_section(&mut out, 10, &section);
        }

        // 11: data
        if !self.data.is_empty() {
            let mut section = Vec::new();
            uleb(&mut section, self.data.len() as u64);
            for (offset, bytes) in &self.data {
                uleb(&mut section, 0); // active, memory 0
                section.push(0x41); // i32.const
                sleb(&mut section, i64::from(*offset));
                section.push(0x0B); // end
                uleb(&mut section, bytes.len() as u64);
                section.extend_from_slice(bytes);
            }
            emit_section(&mut out, 11, &section);
        }

        // 0: custom — the `name` section (function names, for anyone
        // disassembling what we emit) plus whatever the backend attaches.
        if !self.func_names.is_empty() {
            let mut payload = Vec::new();
            name_bytes(&mut payload, "name");
            let mut subsection = Vec::new();
            uleb(&mut subsection, self.func_names.len() as u64);
            for (index, name) in &self.func_names {
                uleb(&mut subsection, u64::from(*index));
                name_bytes(&mut subsection, name);
            }
            payload.push(0x01); // subsection 1: function names
            uleb(&mut payload, subsection.len() as u64);
            payload.extend_from_slice(&subsection);
            emit_section(&mut out, 0, &payload);
        }
        for (name, bytes) in &self.custom {
            let mut payload = Vec::new();
            name_bytes(&mut payload, name);
            payload.extend_from_slice(bytes);
            emit_section(&mut out, 0, &payload);
        }
        out
    }
}

fn emit_section(out: &mut Vec<u8>, id: u8, contents: &[u8]) {
    out.push(id);
    uleb(out, contents.len() as u64);
    out.extend_from_slice(contents);
}

fn name_bytes(out: &mut Vec<u8>, name: &str) {
    uleb(out, name.len() as u64);
    out.extend_from_slice(name.as_bytes());
}

pub fn uleb(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            return;
        }
    }
}

pub fn sleb(out: &mut Vec<u8>, mut value: i64) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        let sign_bit_set = byte & 0x40 != 0;
        if (value == 0 && !sign_bit_set) || (value == -1 && sign_bit_set) {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

/// One function body under construction: extra locals plus the
/// instruction stream. Locals are declared through [`FuncBody::local`],
/// which hands back the index in the combined (params ++ locals) space.
pub struct FuncBody {
    param_count: u32,
    locals: Vec<ValType>,
    code: Vec<u8>,
}

impl FuncBody {
    pub fn new(param_count: u32) -> FuncBody {
        FuncBody {
            param_count,
            locals: Vec::new(),
            code: Vec::new(),
        }
    }

    pub fn local(&mut self, ty: ValType) -> u32 {
        self.locals.push(ty);
        self.param_count + self.locals.len() as u32 - 1
    }

    fn finish(self) -> Vec<u8> {
        let mut out = Vec::new();
        // Run-length encode the local declarations.
        let mut runs: Vec<(u32, ValType)> = Vec::new();
        for ty in &self.locals {
            match runs.last_mut() {
                Some((count, run_ty)) if run_ty == ty => *count += 1,
                _ => runs.push((1, *ty)),
            }
        }
        uleb(&mut out, runs.len() as u64);
        for (count, ty) in runs {
            uleb(&mut out, u64::from(count));
            out.push(ty.code());
        }
        out.extend_from_slice(&self.code);
        out.push(0x0B); // end of function
        out
    }

    fn op(&mut self, opcode: u8) {
        self.code.push(opcode);
    }

    fn op_idx(&mut self, opcode: u8, index: u32) {
        self.code.push(opcode);
        uleb(&mut self.code, u64::from(index));
    }

    // --- control -----------------------------------------------------

    pub fn unreachable(&mut self) {
        self.op(0x00);
    }

    /// `block` with no result type.
    pub fn block_void(&mut self) {
        self.op(0x02);
        self.code.push(0x40);
    }

    /// `loop` with no result type.
    pub fn loop_void(&mut self) {
        self.op(0x03);
        self.code.push(0x40);
    }

    /// `if` with one `i64` result — the shape every value-producing
    /// conditional in this backend uses.
    pub fn if_i64(&mut self) {
        self.op(0x04);
        self.code.push(ValType::I64.code());
    }

    /// `if` with no result.
    pub fn if_void(&mut self) {
        self.op(0x04);
        self.code.push(0x40);
    }

    pub fn else_(&mut self) {
        self.op(0x05);
    }

    pub fn end(&mut self) {
        self.op(0x0B);
    }

    pub fn br(&mut self, depth: u32) {
        self.op_idx(0x0C, depth);
    }

    pub fn br_if(&mut self, depth: u32) {
        self.op_idx(0x0D, depth);
    }

    pub fn br_table(&mut self, targets: &[u32], default: u32) {
        self.op(0x0E);
        uleb(&mut self.code, targets.len() as u64);
        for target in targets {
            uleb(&mut self.code, u64::from(*target));
        }
        uleb(&mut self.code, u64::from(default));
    }

    pub fn return_(&mut self) {
        self.op(0x0F);
    }

    pub fn call(&mut self, func: u32) {
        self.op_idx(0x10, func);
    }

    // --- parametric / variables --------------------------------------

    pub fn drop_(&mut self) {
        self.op(0x1A);
    }

    pub fn select(&mut self) {
        self.op(0x1B);
    }

    pub fn local_get(&mut self, index: u32) {
        self.op_idx(0x20, index);
    }

    pub fn local_set(&mut self, index: u32) {
        self.op_idx(0x21, index);
    }

    pub fn local_tee(&mut self, index: u32) {
        self.op_idx(0x22, index);
    }

    pub fn global_get(&mut self, index: u32) {
        self.op_idx(0x23, index);
    }

    pub fn global_set(&mut self, index: u32) {
        self.op_idx(0x24, index);
    }

    // --- memory ------------------------------------------------------

    /// `i32.load8_u` with natural alignment and no static offset.
    pub fn i32_load8_u(&mut self) {
        self.op(0x2D);
        uleb(&mut self.code, 0); // align
        uleb(&mut self.code, 0); // offset
    }

    // --- constants ---------------------------------------------------

    pub fn i32_const(&mut self, value: i32) {
        self.op(0x41);
        sleb(&mut self.code, i64::from(value));
    }

    pub fn i64_const(&mut self, value: i64) {
        self.op(0x42);
        sleb(&mut self.code, value);
    }

    // --- i32 ---------------------------------------------------------

    pub fn i32_eqz(&mut self) {
        self.op(0x45);
    }

    pub fn i32_eq(&mut self) {
        self.op(0x46);
    }

    pub fn i32_ne(&mut self) {
        self.op(0x47);
    }

    pub fn i32_lt_u(&mut self) {
        self.op(0x49);
    }

    pub fn i32_add(&mut self) {
        self.op(0x6A);
    }

    // --- i64 ---------------------------------------------------------

    pub fn i64_eqz(&mut self) {
        self.op(0x50);
    }

    pub fn i64_eq(&mut self) {
        self.op(0x51);
    }

    pub fn i64_ne(&mut self) {
        self.op(0x52);
    }

    pub fn i64_lt_s(&mut self) {
        self.op(0x53);
    }

    pub fn i64_lt_u(&mut self) {
        self.op(0x54);
    }

    pub fn i64_gt_s(&mut self) {
        self.op(0x55);
    }

    pub fn i64_gt_u(&mut self) {
        self.op(0x56);
    }

    pub fn i64_le_s(&mut self) {
        self.op(0x57);
    }

    pub fn i64_le_u(&mut self) {
        self.op(0x58);
    }

    pub fn i64_ge_s(&mut self) {
        self.op(0x59);
    }

    pub fn i64_ge_u(&mut self) {
        self.op(0x5A);
    }

    pub fn i64_add(&mut self) {
        self.op(0x7C);
    }

    pub fn i64_sub(&mut self) {
        self.op(0x7D);
    }

    pub fn i64_mul(&mut self) {
        self.op(0x7E);
    }

    pub fn i64_div_s(&mut self) {
        self.op(0x7F);
    }

    pub fn i64_div_u(&mut self) {
        self.op(0x80);
    }

    pub fn i64_and(&mut self) {
        self.op(0x83);
    }

    pub fn i64_or(&mut self) {
        self.op(0x84);
    }

    pub fn i64_xor(&mut self) {
        self.op(0x85);
    }

    // --- conversions -------------------------------------------------

    pub fn i32_wrap_i64(&mut self) {
        self.op(0xA7);
    }

    pub fn i64_extend_i32_u(&mut self) {
        self.op(0xAD);
    }
}
