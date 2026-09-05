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
