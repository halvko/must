//! What the emitted module IS, as opposed to what it computes.
//!
//! Three claims P06 makes are structural, so they are pinned structurally
//! rather than argued in prose:
//!
//! 1. **No runtime.** The only thing the module needs from its host is the
//!    platform effect it declares — `must.print`. No allocator, no
//!    unwinder, no support library, no start function, no tables.
//! 2. **Monomorphization is real.** One generic body becomes one wasm
//!    function per instantiation, named deterministically, and a trait
//!    dictionary leaves no runtime trace at all.
//! 3. **Compilation is deterministic.** The same program compiles to the
//!    same bytes, twice in a row and in a fresh database.

mod harness;

use base_db::RootDatabase;

fn compile(source: &str, entry: &str) -> codegen_wasm::Artifact {
    harness::on_budget(|| {
        let db = RootDatabase::default();
        let loc = harness::prepare(&db, source, entry);
        codegen_wasm::compile(&db, &loc).expect("compiles")
    })
}

const TRAITS: &str = r#"
trait Write = requires {
    push: fn(s: str, w: Self) -> Self;
};
trait Display = requires {
    fmt: fn::<W: Write>(w: W, x: Self) -> W;
} with {
    impl str {
        fmt = fn::<W: Write>(w: W, x: str) -> W { w.push(x) };
    }
    impl usize {
        fmt = fn::<W: Write>(w: W, x: usize) -> W { w.push("n") };
    }
};
type Sink = struct { pushes: usize } with {
    impl Write {
        push = fn (s: str, w: Self) -> Self { print(s); Sink(struct { pushes = w.pushes + 1 }) };
    }
};
static show = fn::<T: Display>(x: T) -> usize {
    let s = Sink(struct { pushes = 0 });
    let s = x.fmt(s);
    s.pushes
};
static main = fn () -> usize {
    let n: usize = 1;
    show::<str>("a") + show::<usize>(n)
};
"#;

#[test]
fn the_module_imports_nothing_but_the_platform_effect() {
    let artifact = compile(TRAITS, "main()");
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, &artifact.wasm[..]).expect("validates");
    let imports: Vec<(String, String)> = module
        .imports()
        .map(|import| (import.module().to_owned(), import.name().to_owned()))
        .collect();
    assert_eq!(
        imports,
        vec![("must".to_owned(), "print".to_owned())],
        "a program with no runtime imports exactly its effects — nothing else"
    );
}

#[test]
fn the_module_exports_only_its_entry_point_and_its_reporting_surface() {
    let artifact = compile(TRAITS, "main()");
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, &artifact.wasm[..]).expect("validates");
    let mut exports: Vec<String> = module.exports().map(|e| e.name().to_owned()).collect();
    exports.sort();
    assert_eq!(
        exports,
        vec![
            "main".to_owned(),
            "memory".to_owned(),
            "panic_message_len".to_owned(),
            "panic_message_offset".to_owned(),
            "trap_code".to_owned(),
        ]
    );
}

#[test]
fn a_generic_body_becomes_one_function_per_instantiation() {
    let artifact = compile(TRAITS, "main()");
    let names = String::from_utf8_lossy(&artifact.wasm).into_owned();
    // The `name` custom section carries the mangled instance names, so the
    // specialization is visible in the artifact itself.
    assert!(
        names.contains("show$b0::<str"),
        "`show::<str>` must be its own function"
    );
    assert!(
        names.contains("show$b0::<usize"),
        "`show::<usize>` must be its own function"
    );
    // Both implementations of `fmt` were reached through a dictionary
    // parameter, and both exist as ordinary functions of their own — the
    // dictionary entry is part of the instance key, which is what "static
    // dispatch made literal" means.
    assert!(
        names.contains("Display::str::fmt$b0"),
        "the `str` impl compiles to its own function"
    );
    assert!(
        names.contains("Display::usize::fmt$b0"),
        "and so does the `usize` impl"
    );
    assert!(
        names.contains("p1=str::fmt$b0"),
        "the instance key records WHICH dictionary was passed"
    );
}

#[test]
fn a_dictionary_parameter_has_no_runtime_representation() {
    // `show::<T: Display>(x: T)` takes ONE written parameter and one
    // hidden dictionary parameter. `show::<str>` must therefore have
    // exactly the two slots a `str` occupies — the dictionary is gone.
    let artifact = compile(
        r#"
trait Display = requires {
    fmt: fn(x: Self) -> str;
} with {
    impl str {
        fmt = fn (x: str) -> str { x };
    }
};
static show = fn::<T: Display>(x: T) -> str { x.fmt() };
static main = fn () -> str { show::<str>("a") };
"#,
        "show::<str>(\"a\")",
    );
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, &artifact.wasm[..]).expect("validates");
    let mut store = wasmi::Store::new(&engine, ());
    let mut linker = wasmi::Linker::<()>::new(&engine);
    linker
        .func_wrap("must", "print", |_: i32, _: i32| {})
        .expect("define print");
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .expect("instantiate");
    // The entry returns a `str`: two slots, and no dictionary anywhere.
    let main = instance.get_func(&store, "main").expect("exported");
    assert_eq!(main.ty(&store).params().len(), 0);
    assert_eq!(main.ty(&store).results().len(), 2);
}

#[test]
fn compilation_is_deterministic() {
    let first = compile(TRAITS, "main()");
    let second = compile(TRAITS, "main()");
    assert_eq!(
        first.wasm, second.wasm,
        "the same program must compile to the same bytes: instance discovery, \
         mangling, string interning and the trap table are all order-stable"
    );
    assert_eq!(first.instances, second.instances);
}

#[test]
fn the_trap_table_travels_with_the_module() {
    // A host without this crate can still say which trap fired: the table
    // is in a custom section.
    let artifact = compile(
        r#"static main = fn () -> usize { panic("explicit") };"#,
        "main()",
    );
    let text = String::from_utf8_lossy(&artifact.wasm).into_owned();
    assert!(text.contains("must.traps"), "the custom section is present");
    assert!(text.contains("explicit"), "with the trap's own message");
}

#[test]
fn a_runtime_panic_message_is_readable_after_the_trap() {
    // The message is not a literal at the call, so it is published through
    // the module's globals rather than baked into the trap table.
    harness::check(
        r#"
static pick = fn (n: usize) -> str { if n == 0 { "zero" } else { "other" } };
static main = fn () -> usize { panic(pick(1)) };
"#,
        "main()",
    );
}

#[test]
fn polymorphic_recursion_is_refused_by_name() {
    // `go::<T>` calls itself at `Option::<T>`, so every call needs an
    // instance the previous one did not: monomorphization has nothing to
    // bottom out on. It must be a located refusal like any other, never a
    // blown stack.
    let message = harness::refusal(
        r#"
type Option = enum::<T> { Some(T), None };
static go = const fn::<T>(n: usize, t: T) -> usize {
    if n == 0 { 0 } else { go::<Option::<T>>(n - 1, Option::Some(t)) }
};
static main = fn () -> usize { go::<usize>(3, 1) };
"#,
        "main()",
    );
    assert!(
        message.starts_with("instantiation depth exceeded at `go::<…>`"),
        "the refusal names the runaway instantiation: {message}"
    );
    assert!(
        message.contains("polymorphic recursion"),
        "and says what the shape is: {message}"
    );
}

#[test]
fn a_long_chain_of_distinct_non_generic_functions_is_not_polymorphic_recursion() {
    // The depth guard above counts RE-entries on the current path — an
    // item that already sits earlier on it — not the path's raw length:
    // 200 distinct, non-generic functions calling one another in a chain
    // is ordinary code (each registers exactly once, so nothing on the
    // path ever repeats) and must compile, not be mistaken for the
    // unbounded-instantiation shape above.
    let mut source = String::from("static f0 = fn (x: usize) -> usize { x };\n");
    for n in 1..200 {
        source.push_str(&format!(
            "static f{n} = fn (x: usize) -> usize {{ f{}(x) + 1 }};\n",
            n - 1
        ));
    }
    source.push_str("static main = fn () -> usize { f199(0) };\n");
    compile(&source, "main()");
}

#[test]
fn mutual_polymorphic_recursion_is_refused_by_name() {
    // `a::<T>` calls `b::<T>`, which calls `a::<Option::<T>>`: a 2-item
    // cycle where neither item alone ever recurs with a fresh
    // instantiation, only the pair together. Counting RE-entries of
    // either item catches it after roughly 128 frames, the same order of
    // magnitude as the single-item case. A cycle long enough to reach the
    // path cap before its 128th re-entry is refused by that cap instead —
    // see `a_generic_cycle_at_the_caps_is_refused_by_name`.
    let message = harness::refusal(
        r#"
type Option = enum::<T> { Some(T), None };
static a = const fn::<T>(n: usize, t: T) -> usize {
    if n == 0 { 0 } else { b::<T>(n - 1, t) }
};
static b = const fn::<T>(n: usize, t: T) -> usize {
    if n == 0 { 0 } else { a::<Option::<T>>(n - 1, Option::Some(t)) }
};
static main = fn () -> usize { a::<usize>(3, 1) };
"#,
        "main()",
    );
    assert!(
        message.contains("polymorphic recursion"),
        "and says what the shape is: {message}"
    );
}

#[test]
fn a_very_deep_chain_of_distinct_functions_is_refused_as_too_deep() {
    // No item here ever recurs, so the re-entry-based guard above sees
    // nothing to count; this shape needs its own absolute cap on the
    // path's raw length instead, worded as a depth limit rather than a
    // polymorphic-recursion diagnosis (nothing here is polymorphic).
    let mut source = String::from("static f0 = fn (x: usize) -> usize { x };\n");
    for n in 1..250 {
        source.push_str(&format!(
            "static f{n} = fn (x: usize) -> usize {{ f{}(x) + 1 }};\n",
            n - 1
        ));
    }
    source.push_str("static main = fn () -> usize { f249(0) };\n");
    // Half the budget, for the reason the generic-cycle test below
    // gives: a depth cap is only honest if the stack it was calibrated
    // against still holds the walk it permits.
    let message = harness::refusal_on_stack(codegen_wasm::STACK_BUDGET / 2, &source, "main()");
    assert!(
        message.contains("call chain deeper than"),
        "names the shape as a depth limit, not polymorphic recursion: {message}"
    );
    // `Refusal::message` appends " is not supported by the wasm backend
    // yet", so what the cap names has to be a noun phrase that survives
    // the suffix.
    assert!(
        message.ends_with("a call graph this deep is not supported by the wasm backend yet"),
        "the composed message reads as one sentence: {message}"
    );
}

#[test]
fn a_generic_cycle_at_the_caps_is_refused_by_name() {
    // The heaviest shape the walk can be handed, at the deepest point the
    // caps let it reach: a mutual cycle of 92 generic items, every step
    // re-instantiating the next at a fresh type. A cycle this long fills
    // the path's 220-frame ceiling before it has collected its 128th
    // re-entry, so it is the path cap that fires; either way the walk
    // runs as deep as it is ever allowed to. It must come back with a
    // refusal that names the shape — and on HALF the stack budget
    // `compile` documents, which is the margin `MAX_PATH_DEPTH`'s doc
    // claims for exactly this shape.
    const K: usize = 92;
    let mut source = String::from("type Option = enum::<T> { Some(T), None };\n");
    for n in 0..K {
        let body = if n + 1 == K {
            "f0::<Option::<T>>(n - 1, Option::Some(t))".to_owned()
        } else {
            format!("f{}::<T>(n - 1, t)", n + 1)
        };
        source.push_str(&format!(
            "static f{n} = const fn::<T>(n: usize, t: T) -> usize \
             {{ if n == 0 {{ 0 }} else {{ {body} }} }};\n"
        ));
    }
    source.push_str("static main = fn () -> usize { f0::<usize>(3, 1) };\n");
    let message = harness::refusal_on_stack(codegen_wasm::STACK_BUDGET / 2, &source, "main()");
    assert!(
        message.contains("polymorphic recursion") || message.contains("call chain deeper than"),
        "one of the two monomorphization caps names it: {message}"
    );
    assert!(
        message.ends_with("is not supported by the wasm backend yet"),
        "and reads as one sentence: {message}"
    );
}

#[test]
fn a_declared_host_import_becomes_a_real_wasm_import_and_is_called_through() {
    // The host-import mechanism, end to end at the module level: the
    // declaration's own name is the import's field name (module `must`,
    // `print`'s sibling), it lands in the import section, and the call
    // goes through it. Nothing here is `read`-specific — `read` itself
    // needs a raw pointer, which this backend refuses by name.
    let source = "extern static host_tick: unsafe fn(n: i64) -> i64;\n\
                  static main = fn () -> i64 { unsafe { host_tick(7) } };";
    let artifact = compile(source, "main()");
    let engine = wasmi::Engine::default();
    let module = wasmi::Module::new(&engine, &artifact.wasm[..]).expect("validates");
    let imports: Vec<(String, String)> = module
        .imports()
        .map(|import| (import.module().to_owned(), import.name().to_owned()))
        .collect();
    assert_eq!(
        imports,
        vec![
            ("must".to_owned(), "print".to_owned()),
            ("must".to_owned(), "host_tick".to_owned()),
        ],
        "a declared import joins `print` in the import section, under its own name"
    );

    // And it really is wired: a host that doubles gets 7 and the module
    // returns 14.
    let mut store = wasmi::Store::new(&engine, ());
    let mut linker = wasmi::Linker::new(&engine);
    linker
        .func_wrap("must", "print", |_: i32, _: i32| {})
        .expect("print");
    linker
        .func_wrap("must", "host_tick", |n: i64| n * 2)
        .expect("host_tick");
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .expect("instantiates");
    let entry = instance
        .get_typed_func::<(), i64>(&store, "main")
        .expect("entry");
    assert_eq!(entry.call(&mut store, ()).expect("runs"), 14);
}

#[test]
fn an_import_whose_signature_has_no_wasm_shape_is_refused_by_name() {
    // The `read` primitive itself, on this backend: its buffer parameter
    // is a raw pointer, and pointers are out of scope here. The refusal
    // names the IMPORT — which boundary is unavailable is the useful half.
    let message = harness::on_budget(|| {
        let db = RootDatabase::default();
        let source = "extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> i64;\n\
                      static main = fn () -> i64 {\n\
                          let mut b: u8 = 0;\n\
                          unsafe { read(b.&raw mut, 1) }\n\
                      };";
        let loc = harness::prepare(&db, source, "main()");
        let Err(codegen_wasm::CompileError::Unsupported(refusal)) =
            codegen_wasm::compile(&db, &loc)
        else {
            panic!("the backend must refuse an import it cannot lay out");
        };
        refusal.message()
    });
    assert_eq!(
        message,
        "a raw pointer (heap and pointer primitives are out of scope for this backend) \
         in the host import `read`'s signature is not supported by the wasm backend yet"
    );
}

#[test]
fn a_bound_host_import_compiles_but_a_stored_one_is_refused_by_name() {
    // `let f = read;` is ORDINARY source since unsafety moved into the
    // type — it used to need an `unsafe` marker — so what the backend does
    // with a function value became a question people can reach by writing
    // something perfectly legal. Both halves pinned, because the honest
    // statement is a pair and not a slogan:
    //
    // a binding compiles (the static-value tracker follows it to the one
    // import it can only be, and the call is a direct call), and a value
    // the tracker CANNOT follow — stored in a record, or merged from
    // branches that disagree — is refused BY NAME rather than miscompiled.
    let db = RootDatabase::default();
    let bound = "extern static tick: unsafe fn(n: i64) -> i64;\n\
                 static main = fn () -> i64 { let f = tick; unsafe { f(1) } };";
    let loc = harness::prepare(&db, bound, "main()");
    codegen_wasm::compile(&db, &loc).expect("a bound import is still a direct call");

    let db = RootDatabase::default();
    let stored = "extern static tick: unsafe fn(n: i64) -> i64;\n\
                  static main = fn () -> i64 { \
                      let h = struct { go = tick }; unsafe { h.go(1) } \
                  };";
    let loc = harness::prepare(&db, stored, "main()");
    let Err(codegen_wasm::CompileError::Unsupported(refusal)) = codegen_wasm::compile(&db, &loc)
    else {
        panic!("the backend must refuse a call through a value it cannot follow");
    };
    assert_eq!(
        refusal.message(),
        "a call through a function value that is not statically known \
         (function values stored in data, or merged from several branches) \
         is not supported by the wasm backend yet"
    );
}

#[test]
fn an_import_may_not_claim_a_name_the_compiler_already_imports() {
    // Two imports of one `(module, field)` is a module with two answers to
    // the same question — an engine resolves BOTH, so the program runs with
    // whichever the host happened to bind, silently. That is a wrong-answer
    // class, not a missing feature, so it is rejected rather than "not
    // supported yet". The reserved set is the module's own import list at
    // this point, so a future sibling of `print` reserves itself with no
    // second place to write it down — provided it is declared above the
    // call that collects the program's imports.
    let (message, named) = harness::on_budget(|| {
        let db = RootDatabase::default();
        let source = "extern static print: unsafe fn(offset: i64, len: i64) -> ();\n\
                      static main = fn () -> () { unsafe { print(0, 0) }; };";
        let loc = harness::prepare(&db, source, "main()");
        let Err(err) = codegen_wasm::compile(&db, &loc) else {
            panic!("the backend must reject an import that collides with `must.print`");
        };
        let (item, _) = err.origin().expect("the rejection carries a caret");
        (err.message(), item.display_name().to_owned())
    });
    assert_eq!(
        message,
        "an import may not be named `print`: this backend already imports \
         `must.print` for the builtin of that name, and a module cannot import \
         one name twice"
    );
    assert_eq!(
        named, "print",
        "the rejection must point at the declaration it asks the user to rename, \
         not at the call that reached it"
    );
}

/// Linearity is CHECK-TIME ONLY. The `forget` capability decides which
/// programs the checker accepts and nothing else: there is no drop glue to
/// emit, no flag to track, no cleanup block to branch to. The claim is
/// structural, so it is pinned structurally — the same program with and
/// without the ceiling must compile to the same bytes, down to the last
/// one.
#[test]
fn a_linear_type_changes_nothing_about_the_emitted_module() {
    const PROGRAM: &str = r#"
type Res = struct { id: usize, size: usize }PLACEHOLDER with {
    impl Self {
        drop = fn(r: Self) -> usize {
            let Res(struct { id, size }) = r;
            id + size
        };
    }
};
static main = fn () -> usize {
    let r = Res(struct { id = 3, size = 4 });
    r.drop()
};
"#;
    let linear = compile(&PROGRAM.replace("PLACEHOLDER", " only move"), "main()");
    let plain = compile(&PROGRAM.replace("PLACEHOLDER", ""), "main()");
    assert_eq!(
        linear.wasm, plain.wasm,
        "a checked-linear type must have no codegen consequence at all"
    );
    assert_eq!(linear.instances, plain.instances);
}

/// `unsafe fn` is CHECK-TIME ONLY, by the same argument and pinned the same
/// way. The flag decides which programs the checker accepts — where an
/// `unsafe { ... }` block is required, and which coercion is legal — and
/// codegen never learns a function type carried it: there is no second
/// calling convention, no wrapper, no tag on a function value. Same program
/// with and without the marker, same bytes.
#[test]
fn an_unsafe_fn_type_changes_nothing_about_the_emitted_module() {
    const PROGRAM: &str = r#"
static twice = fn (x: usize) -> usize { x + x };
static apply = fn (g: PLACEHOLDERfn(usize) -> usize, x: usize) -> usize {
    unsafe { g(x) }
};
static main = fn () -> usize { apply(twice, 21) };
"#;
    let priced = compile(&PROGRAM.replace("PLACEHOLDER", "unsafe "), "main()");
    let plain = compile(&PROGRAM.replace("PLACEHOLDER", ""), "main()");
    assert_eq!(
        priced.wasm, plain.wasm,
        "unsafety in a function type must have no codegen consequence at all"
    );
    assert_eq!(priced.instances, plain.instances);
}
