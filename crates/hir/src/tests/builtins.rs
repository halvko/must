//! Compiler-provided declarations: builtin enums, `read_line`, `next_char`, the blesses.

use super::*;

#[test]
fn every_compiler_provided_enum_resolves_with_its_variants() {
    // The end-to-end half of the table's contract: each name resolves in
    // ANNOTATION position without the file declaring it, and each carries
    // exactly its own variants with exactly their payloads — a dropped row
    // makes the annotation unresolved, a wrong variant list makes the
    // exhaustive `match` complain. Named enums on purpose, so a fixture
    // reads like the Must a user writes;
    // `every_table_row_is_registered_and_declared` covers whatever ROWS the
    // table happens to hold, and the per-enum shadowing tests pin the other
    // half of the rule (a user declaration wins).
    check_diagnostics(
        r#"
static f = fn (
    a: AllocResult::<usize>,
    b: ReadLineResult,
    c: NextChar,
    d: Utf8Result,
) -> usize {
    let w = match a { ::Ok(p) => unsafe { p.* }, ::Err => 0 };
    let x = match b { ::Line(line) => line.len(), ::End => 0 };
    let y = match c { ::Char(ch, next) => next, ::End => 0 };
    let z = match d { ::Ok(text) => text.len(), ::Err => 0 };
    w + x + y + z
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn every_file_names_the_same_compiler_provided_declarations() {
    let db = RootDatabase::default();
    let a = SourceFile::new(&db, "a.must".to_owned(), String::new());
    let b = SourceFile::new(&db, "b.must".to_owned(), String::new());
    for decl in crate::scopes::synthetic_decls() {
        let prelude = Some(crate::Resolution::TypeItem(crate::synthetic_decl_loc(
            &db, decl.name,
        )));
        for file in [a, b] {
            assert_eq!(crate::type_scope(&db, file).resolve(decl.name), prelude);
            assert_eq!(crate::file_scope(&db, file).resolve(decl.name), prelude);
        }
    }
}

#[test]
fn every_table_row_is_registered_and_declared() {
    // The addition half of the table's contract, at the Rust level so that
    // it covers ROWS and not four hand-named enums: whatever
    // `synthetic_decls` holds, every row is in both scopes of an ordinary
    // file under its own reserved location, `type_decl` hands back exactly
    // the row's variants, and `item_data` hands back exactly its binder. A
    // fifth row that nothing registered fails here without anyone having to
    // think to write a fixture for it.
    let db = RootDatabase::default();
    let file = SourceFile::new(
        &db,
        "test.must".to_owned(),
        "static f = fn () {};".to_owned(),
    );
    let table = crate::scopes::synthetic_decls();
    assert!(!table.is_empty(), "the table is what this test is about");
    for decl in table.iter() {
        let loc = crate::synthetic_decl_loc(&db, decl.name);
        assert_eq!(
            crate::type_scope(&db, file).resolve(decl.name),
            Some(crate::Resolution::TypeItem(loc.clone())),
            "`{}` is missing from the type scope",
            decl.name
        );
        assert_eq!(
            crate::file_scope(&db, file).resolve(decl.name),
            Some(crate::Resolution::TypeItem(loc.clone())),
            "`{}` is missing from the file scope",
            decl.name
        );
        let item = loc.to_id(&db);
        assert_eq!(
            crate::type_decl(&db, item),
            &Some(crate::TypeDeclData::Enum {
                variants: decl.variants.clone(),
            }),
            "`{}`'s declared shape is not its row's",
            decl.name
        );
        let data = crate::item_data(&db, item)
            .as_ref()
            .expect("a compiler-provided enum has item data");
        assert_eq!(data.generics, decl.generics, "`{}`'s binder", decl.name);
        assert_eq!(data.kind, crate::ItemKind::Type, "`{}`'s kind", decl.name);
    }
}

#[test]
fn a_value_item_taking_a_builtin_enum_name_takes_the_type_with_it() {
    // The cross-kind edge of shadowing, pinned as the honest behavior it
    // is: both registrations ask only whether the NAME is declared, never
    // by what KIND of item, so a `static NextChar` takes the name out of
    // the type scope as thoroughly as a `type NextChar` would — and a value
    // item declares no type, so the annotation then resolves to nothing at
    // all — reported as the ordinary "not a type", exactly as it would be
    // for a user's own `static NextChar` shadowing a user's own `type`. The
    // `print` precedent read consistently (a file that spells the name owns
    // it); the alternative, letting a value item and a compiler-provided
    // type share a name, is a rule about KINDS that nothing else in the
    // language has yet.
    check_diagnostics(
        r#"
static NextChar = 'x';
static f = fn (c: NextChar) -> usize { 1 };
"#,
        expect![[r#"
            42..50: `NextChar` is not a type
        "#]],
    );
}

#[test]
fn read_line_result_can_be_matched_in_the_documented_idiom() {
    // The exact idiom `read_line`'s doc comment shows: `loop { match
    // read_line() { ::Line(s) => ..., ::End => break ... } }` typechecks
    // clean with no annotation anywhere — `ReadLineResult` is an ordinary
    // nominal enum resolved the same way `AllocResult` is.
    check_diagnostics(
        r#"
static count_lines = fn () -> usize {
    let mut n = 0;
    loop {
        match read_line() {
            ::Line(s) => { n = n + 1; },
            ::End => break n,
        }
    }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn read_line_is_a_nullary_first_class_function() {
    // Bare mention (no call): `read_line`'s type names the prelude's
    // `ReadLineResult` decl, exactly like `print`'s bare mention names its
    // fixed `fn(str) -> ()`.
    check_infer(
        r#"
static f = fn () {
    let g = read_line;
};
"#,
        expect![[r#"
            12..44 'fn () {     let g...': fn()
            18..44 '{     let g = rea...': ()
            28..29 'g': fn() -> ReadLineResult
            32..41 'read_line': fn() -> ReadLineResult
        "#]],
    );
}

#[test]
fn user_declarations_shadow_the_builtin_read_line_result() {
    // The `AllocResult`/`print` precedent, applied to `read_line`'s result
    // type: a file declaring its own `ReadLineResult` sees its own
    // everywhere.
    check_infer(
        r#"
type ReadLineResult = struct { tag: usize };
static f = fn () -> ReadLineResult {
    ReadLineResult(struct { tag = 1 })
};
"#,
        expect![[r#"
            57..123 'fn () -> ReadLine...': fn() -> ReadLineResult
            81..123 '{     ReadLineRes...': ReadLineResult
            87..101 'ReadLineResult': fn(struct { tag: usize }) -> ReadLineResult
            87..121 'ReadLineResult(st...': ReadLineResult
            102..120 'struct { tag = 1 }': struct { tag: usize }
            117..118 '1': usize
        "#]],
    );
}

#[test]
fn next_char_is_a_builtin_member_of_str() {
    // Reached through the dot on a `str` receiver, so its type has the
    // dot-callable shape every member has — `str` last (TR01) — and it
    // answers the prelude's `NextChar` decl, the way `read_line` answers
    // `ReadLineResult`.
    check_infer(
        r#"
static f = fn (s: str) {
    let n = s.next_char(0);
};
"#,
        expect![[r#"
            12..55 'fn (s: str) {    ...': fn(str)
            16..17 's': str
            24..55 '{     let n = s.n...': ()
            34..35 'n': NextChar
            38..39 's': str
            38..49 's.next_char': fn(usize, str) -> NextChar
            38..52 's.next_char(0)': NextChar
            50..51 '0': usize
        "#]],
    );
}

#[test]
fn a_builtin_member_call_counts_only_the_written_arguments() {
    // The receiver supplies one parameter, so arity is reported against
    // what was written. Not a rule of its own: this is the shared
    // receiver-appending tail, down to the follow-on mismatch an extra
    // argument gets from landing in the receiver's slot.
    check_diagnostics(
        r#"
type Cell = struct { v: usize } with {
    impl Self {
        plus = fn(extra: usize, c: Self) -> usize { c.v + extra };
    }
};
static f = fn (s: str, c: Cell) {
    let a = s.next_char();
    let b = s.next_char(0, 1);
    let d = c.plus();
    let e = c.plus(0, 1);
};
"#,
        expect![[r#"
            178..191: expected 1 argument(s), found 0
            205..222: expected 1 argument(s), found 2
            220..221: type mismatch: expected `str`, found `{number}`
            236..244: expected 1 argument(s), found 0
            258..270: expected 1 argument(s), found 2
            268..269: type mismatch: expected `Cell`, found `{number}`
        "#]],
    );
}

#[test]
fn next_char_walks_a_string_in_the_documented_idiom() {
    // The idiom `NextChar`'s doc comment shows: thread the index back in,
    // stop at `::End`. Typechecks with no annotation anywhere.
    check_diagnostics(
        r#"
static count = fn (s: str) -> usize {
    let mut n = 0;
    let mut i = 0;
    loop {
        match s.next_char(i) {
            ::Char(c, next) => { n = n + 1; i = next; },
            ::End => break n,
        }
    }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn next_char_is_not_a_top_level_name() {
    // The member is reachable only through the dot: `next_char` claims
    // nothing in the value namespace, so a bare mention is an ordinary
    // unresolved name and a user's own `next_char` item is unshadowed.
    check_diagnostics(
        r#"
static f = fn (s: str) { next_char(s, 0); };
"#,
        expect![[r#"
            26..35: unresolved name `next_char`
        "#]],
    );
}

#[test]
fn next_char_is_only_a_member_of_str() {
    // Not a member of every type: a non-`str` receiver gets the ordinary
    // "no such member" story, not a special case.
    check_diagnostics(
        r#"
static f = fn (n: usize) { n.next_char(0); };
"#,
        expect![[r#"
            28..42: no field or member `next_char` on `usize`
        "#]],
    );
}

#[test]
fn user_declarations_shadow_the_builtin_next_char_enum() {
    // The `AllocResult`/`ReadLineResult` precedent once more: a file
    // declaring its own `NextChar` sees its own everywhere.
    check_infer(
        r#"
type NextChar = struct { tag: usize };
static f = fn () -> NextChar {
    NextChar(struct { tag = 1 })
};
"#,
        expect![[r#"
            51..105 'fn () -> NextChar...': fn() -> NextChar
            69..105 '{     NextChar(st...': NextChar
            75..83 'NextChar': fn(struct { tag: usize }) -> NextChar
            75..103 'NextChar(struct {...': NextChar
            84..102 'struct { tag = 1 }': struct { tag: usize }
            99..100 '1': usize
        "#]],
    );
}

#[test]
fn next_char_is_const_legal() {
    // Decoding a `str` is pure, so there is no effect for a const context
    // to refuse — the pointer builtins' reason, reached through the dot.
    check_diagnostics(
        r#"
static first = const {
    match "ab".next_char(0) {
        ::Char(c, next) => next,
        ::End => 0,
    }
};
"#,
        expect![[r#""#]],
    );
}

#[test]
fn every_builtin_answers_its_properties() {
    use crate::{Builtin, ConstLegality};

    // The property TABLE, written out: what each builtin answers when
    // asked whether const evaluation accepts it (and if not, why), whether
    // calling it needs `unsafe`, and whether it takes a pointer argument
    // in both raw flavors. All three properties are EXHAUSTIVE matches on
    // the enum, so it is the compiler, not this test, that forces a new
    // builtin to have answers; what this adds is that a CHANGED answer
    // must be changed twice, deliberately.
    //
    // What it does NOT probe is the wiring — that the checkers actually
    // consult the properties. Nothing can, until a builtin exists whose
    // answers differ from the current members' (both are pure and safe,
    // so a member path that ignored the table entirely would still pass
    // every test in this file). That half is held by construction
    // instead: every path — const_check's named callee, const_check's
    // dot-call via `InferenceResult::builtin_of_call`, unsafe_check's
    // named and dot-call arms alike — ends at `Builtin::const_legality`
    // / `Builtin::requires_unsafe`, and none of them names a builtin.
    const ALL: [Builtin; 14] = [
        Builtin::Print,
        Builtin::Panic,
        Builtin::AllocArray,
        Builtin::DeallocArray,
        Builtin::Add,
        Builtin::Offset,
        Builtin::Copy,
        Builtin::Dangling,
        Builtin::ReadLine,
        Builtin::NextChar,
        Builtin::StrFromUtf8,
        Builtin::StrFromUtf8Unchecked,
        Builtin::StrLen,
        Builtin::StrBytes,
    ];

    // `(const_legality, requires_unsafe, flavor_polymorphic)` per builtin.
    fn expected(builtin: Builtin) -> (ConstLegality, bool, bool) {
        match builtin {
            // The host effects: refused by const evaluation, safe to
            // call, no pointer argument.
            Builtin::Print | Builtin::ReadLine => (ConstLegality::HostEffect, false, false),
            // The heap pair: const-FENCED, not effectful (C04) — and
            // freeing is UNSAFE because it invalidates every pointer into
            // the allocation, while allocating cannot UB. Neither takes a
            // flavor-polymorphic pointer: `alloc_array` takes a count,
            // and `dealloc_array` frees one fixed flavor.
            Builtin::AllocArray => (ConstLegality::HeapFence, false, false),
            Builtin::DeallocArray => (ConstLegality::HeapFence, true, false),
            // Pure and safe: `panic` is the one effect const contexts
            // allow, `dangling` only names an address, and the two `str`
            // MEMBERS read what the receiver already holds. None reads a
            // caller-supplied pointer in either flavor.
            Builtin::Panic | Builtin::Dangling | Builtin::NextChar | Builtin::StrLen => {
                (ConstLegality::Legal, false, false)
            }
            // Const-legal AND unsafe — the two properties are orthogonal:
            // pointer arithmetic and range reads carry preconditions
            // nothing checks, but const evaluation detects every would-be
            // UB as a deterministic trap, so it need not refuse them. All
            // four read a pointer in either raw flavor.
            Builtin::Add
            | Builtin::Offset
            | Builtin::Copy
            | Builtin::StrFromUtf8
            | Builtin::StrFromUtf8Unchecked => (ConstLegality::Legal, true, true),
            // Const-legal and unsafe like the group above, but a WRITE
            // through one fixed flavor (`u8.&raw mut`) rather than a read
            // through either — not flavor-polymorphic.
            Builtin::StrBytes => (ConstLegality::Legal, true, false),
        }
    }

    for builtin in ALL {
        assert_eq!(
            (
                builtin.const_legality(),
                builtin.requires_unsafe(),
                builtin.flavor_polymorphic()
            ),
            expected(builtin),
            "{}: (const_legality, requires_unsafe, flavor_polymorphic)",
            builtin.name()
        );
    }
    // A duplicated entry would silently stand in for a missing one.
    for (i, a) in ALL.iter().enumerate() {
        for b in &ALL[..i] {
            assert_ne!(a, b, "{} listed twice", a.name());
        }
    }
}

#[test]
fn a_character_const_argument_type_checks_by_kind() {
    // `char` is in the annotation-representable const domain because it
    // fell out of the same machinery `usize`/`str`/`bool` use, not because
    // it was carved in — so the mismatches have to be checked rather than
    // assumed. An integer, a string and a MALFORMED literal each get their
    // own answer; the malformed one is silent here because the lexer
    // already said what is wrong with it (errors are infectious and
    // silent, never doubled).
    check_diagnostics(
        r#"
static pick = const fn::<const C: char>() -> char { C };
static ok = pick::<'x'>();
static from_int = pick::<5>();
static from_str = pick::<"x">();
static from_empty = pick::<''>();
"#,
        expect![[r#"
            110..111: type mismatch: expected `char`, found `{number}`
            141..144: type mismatch: expected `char`, found `str`
            176..178: empty character literal: a character literal holds exactly one character
        "#]],
    );
}

#[test]
fn next_char_through_a_borrow_names_the_deref() {
    // A builtin member takes its receiver BY VALUE, so a borrow does not
    // reach it — that is auto-deref, sealed. The refusal must be the one a
    // value-`Self` USER member gets in the same position (`.*.name`), not
    // "no such member": the member plainly exists, and saying otherwise
    // would send the reader hunting for a spelling instead of a `.*`.
    check_diagnostics(
        r#"
static f = fn::<@a>(r: str.&::<@a>) -> () {
    let n = r.next_char(0);
};
"#,
        expect![[r#"
            57..68: `str.&::<@a>` is a borrow, so `.next_char` does not reach through it — there is no auto-deref; write `.*.next_char`
        "#]],
    );
}

#[test]
fn a_user_member_shadows_the_builtin_next_char() {
    // Load-bearing in three doc comments: builtin members are consulted
    // only after every user candidate has had its turn, so a `next_char`
    // written in an `impl ... for str` WINS. The return type is the
    // witness — `usize` here, `NextChar` if the builtin had taken it.
    check_infer(
        r#"
trait Chars = requires {
    next_char: fn(usize, Self) -> usize;
} with {
    impl str {
        next_char = fn (i: usize, s: str) -> usize { i };
    }
};
static f = fn (s: str) {
    let n = s.next_char(0);
};
"#,
        expect![[r#"
            169..212 'fn (s: str) {    ...': fn(str)
            173..174 's': str
            181..212 '{     let n = s.n...': ()
            191..192 'n': usize
            195..196 's': str
            195..206 's.next_char': fn(usize, str) -> usize
            195..209 's.next_char(0)': usize
            207..208 '0': usize
        "#]],
    );
}

#[test]
fn dangling_infers_from_the_expected_pointer_type() {
    check_infer(
        r#"
static f = fn {
    let p: str.&raw mut = dangling();
};
"#,
        expect![[r#"
            12..56 'fn {     let p: s...': fn()
            15..56 '{     let p: str....': ()
            25..26 'p': str.&raw mut
            43..51 'dangling': fn() -> str.&raw mut
            43..53 'dangling()': str.&raw mut
        "#]],
    );
}

#[test]
fn unpinned_generic_builtin_mentions_ask_for_a_turbofish() {
    check_diagnostics(
        r#"
static f = fn {
    let r = alloc_array(1);
};
"#,
        expect![[r#"
            29..40: cannot infer the type parameter `T` of `alloc_array`; write `alloc_array::<...>` to specify it
        "#]],
    );
}

// ---- the blesses: bytes into `str` --------------------------------------

#[test]
fn user_declarations_shadow_the_builtin_utf8_result() {
    // The `AllocResult`/`ReadLineResult` rule, once more: a file that
    // declares the name sees its own everywhere.
    check_infer(
        "type Utf8Result = struct { tag: usize };\n\
         static x = Utf8Result(struct { tag = 1 });",
        expect![[r#"
            52..62 'Utf8Result': fn(struct { tag: usize }) -> Utf8Result
            52..82 'Utf8Result(struct...': Utf8Result
            63..81 'struct { tag = 1 }': struct { tag: usize }
            78..79 '1': usize
        "#]],
    );
}

#[test]
fn both_blesses_require_unsafe_and_pin_their_pointee_to_u8() {
    // `unsafe` is about the POINTER in both spellings — that it addresses
    // `len` readable bytes is the caller's unchecked claim, so "checked"
    // names only whether the bytes spell UTF-8. And the pointee is `u8`,
    // not `T`: this boundary is bytes-first, and a bless over some other
    // element type would be a layout claim, not a text one. The first call
    // is the clean one: `p` is a `u8.&raw`, so it also pins that either raw
    // flavor is accepted — the reason neither bless is first-class.
    check_diagnostics(
        "static f = fn(p: u8.&raw, q: usize.&raw mut) -> () {\n\
             unsafe { str_from_utf8(p, 1); };\n\
             str_from_utf8(q, 1);\n\
             str_from_utf8_unchecked(p, 1);\n\
             unsafe { str_from_utf8_unchecked(q, 1); };\n\
         };",
        expect![[r#"
            86..105: calling `str_from_utf8` requires an `unsafe { ... }` block
            100..101: type mismatch: expected `u8.&raw mut`, found `usize.&raw mut`
            107..136: calling `str_from_utf8_unchecked` requires an `unsafe { ... }` block
            171..172: type mismatch: expected `u8.&raw mut`, found `usize.&raw mut`
        "#]],
    );
}

#[test]
fn a_bless_is_not_a_first_class_value() {
    // Same reason `copy` is not: its pointer parameter accepts either raw
    // flavor, which no one `fn` type says.
    check_diagnostics(
        "static f = fn() -> () { let g = str_from_utf8; };",
        expect![[r#"
            32..45: `str_from_utf8` must be called directly; its pointer parameter accepts both `T.&raw` and `T.&raw mut`, so it has no one function type to be a value at
        "#]],
    );
}

#[test]
fn both_blesses_are_const_legal() {
    // A bless has no effect for a const context to refuse — the pointer
    // builtins' reason, and `unsafe` is orthogonal to `const`.
    check_diagnostics(
        "static empty = const { unsafe { str_from_utf8_unchecked(dangling::<u8>(), 0) } };\n\
         static checked = const {\n\
             match unsafe { str_from_utf8(dangling::<u8>(), 0) } {\n\
                 Utf8Result::Ok(s) => s,\n\
                 Utf8Result::Err => \"not utf-8\",\n\
             }\n\
         };",
        expect![""],
    );
}

#[test]
fn str_bytes_requires_unsafe_and_both_str_primitives_are_const_legal() {
    // `str_bytes` is the only one of the two that touches raw memory, and
    // it WRITES: that `dst` addresses `s.len()` writable bytes is the
    // caller's unchecked claim, so it carries `copy`'s destination gate.
    // `len` reads a value it was handed and carries none. Const-legality is
    // orthogonal to `unsafe` and is `next_char`'s reason for both: neither
    // observes anything outside its arguments.
    check_diagnostics(
        "static f = fn(p: u8.&raw mut) -> () { str_bytes(\"hi\", p); };\n\
         static n = const { \"hi\".len() };\n\
         static z = const { unsafe { str_bytes(\"\", dangling::<u8>()) } };",
        expect![[r#"
            38..56: calling `str_bytes` requires an `unsafe { ... }` block
        "#]],
    );
}
