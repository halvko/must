//! The encoder's own proof: a module built by hand here must VALIDATE and
//! RUN on a real engine. Everything the backend relies on structurally —
//! imports, memory + a data segment, multi-value results, a mutable
//! global surviving a trap, `unreachable` — is exercised once, so a
//! format mistake fails here instead of somewhere deep in a differential
//! comparison.

use codegen_wasm::wasm::{ExportKind, FuncBody, Module, ValType};

#[test]
fn a_hand_built_module_validates_and_runs() {
    let mut module = Module::new();
    let print_ty = module.func_type(vec![ValType::I32, ValType::I32], vec![]);
    let print = module.import_func("must", "print", print_ty);

    let main_ty = module.func_type(vec![], vec![ValType::I64, ValType::I64]);
    let main = module.declare_func(main_ty, "main".to_owned());

    module.memory(1);
    module.export("memory", ExportKind::Memory, 0);
    module.export("main", ExportKind::Func, main);
    let trap_code = module.global(ValType::I32, true, -1);
    module.export("trap_code", ExportKind::Global, trap_code);
    module.data_segment(0, b"hello, wasm".to_vec());

    let mut body = FuncBody::new(0);
    // print("hello, wasm")
    body.i32_const(0);
    body.i32_const(11);
    body.call(print);
    // A mutable global written before a (not taken) trap.
    body.i32_const(7);
    body.global_set(trap_code);
    // Multi-value result: (40 + 2, 1 == 1)
    body.i64_const(40);
    body.i64_const(2);
    body.i64_add();
    body.i64_const(1);
    body.i64_const(1);
    body.i64_eq();
    body.i64_extend_i32_u();
    body.return_();
    module.set_body(main, body);

    let bytes = module.finish();
    let (results, printed, trap) = run(&bytes, "main", 2);
    assert_eq!(printed, "hello, wasm");
    assert_eq!(results, vec![42, 1]);
    assert!(trap.is_none());
}

#[test]
fn a_trap_reports_its_code_through_the_global() {
    let mut module = Module::new();
    let main_ty = module.func_type(vec![], vec![ValType::I64]);
    let main = module.declare_func(main_ty, "main".to_owned());
    module.memory(1);
    module.export("memory", ExportKind::Memory, 0);
    module.export("main", ExportKind::Func, main);
    let trap_code = module.global(ValType::I32, true, -1);
    module.export("trap_code", ExportKind::Global, trap_code);

    let mut body = FuncBody::new(0);
    // if 1 { trap_code = 3; unreachable } else { 0 }
    body.i64_const(1);
    body.i64_eqz();
    body.if_i64();
    body.i64_const(0);
    body.else_();
    body.i32_const(3);
    body.global_set(trap_code);
    body.unreachable();
    body.end();
    body.return_();
    module.set_body(main, body);

    let (_, _, trap) = run(&module.finish(), "main", 1);
    assert_eq!(trap, Some(3), "the trap code must survive the trap");
}

/// Instantiate, wire `print` to a capture buffer, call `name`, and report
/// `(results, printed, trap code)`.
fn run(bytes: &[u8], name: &str, results: usize) -> (Vec<i64>, String, Option<i32>) {
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, bytes).expect("the module must validate");
    let mut store = wasmi::Store::new(&engine, Vec::<u8>::new());
    let mut linker = wasmi::Linker::new(&engine);
    linker
        .func_wrap(
            "must",
            "print",
            |mut caller: wasmi::Caller<'_, Vec<u8>>, offset: i32, len: i32| {
                let memory = caller
                    .get_export("memory")
                    .and_then(wasmi::Extern::into_memory)
                    .expect("the module exports its memory");
                let mut buf = vec![0u8; len as usize];
                memory
                    .read(&caller, offset as usize, &mut buf)
                    .expect("print reads inside memory");
                caller.data_mut().extend_from_slice(&buf);
            },
        )
        .expect("define print");
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .expect("instantiate");
    let func = instance
        .get_func(&store, name)
        .expect("the entry point is exported");
    let mut out = vec![wasmi::Val::I64(0); results];
    let outcome = func.call(&mut store, &[], &mut out);
    let trap = match outcome {
        Ok(()) => None,
        Err(_) => {
            let global = instance
                .get_global(&store, "trap_code")
                .expect("trap_code is exported");
            Some(global.get(&store).i32().expect("trap_code is an i32"))
        }
    };
    let values = out
        .iter()
        .map(|value| value.i64().expect("i64 result"))
        .collect();
    (
        values,
        String::from_utf8(store.data().clone()).unwrap(),
        trap,
    )
}
