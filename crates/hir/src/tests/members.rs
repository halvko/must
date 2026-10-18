//! Inherent members and dot-calls.

use super::*;

// ---- inherent members and dot-calls -------------------------------------

#[test]
fn dot_call_happy_path() {
    check_infer(
        r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn(c: Self) -> usize { c.n };
        bump = fn(by: usize, c: Self) -> Self { Counter(struct { n = c.n + by }) };
    }
};
static main = fn() -> usize {
    let c = Counter(struct { n = 3 });
    c.bump(2).get()
};
"#,
        expect![[r#"
            210..286 'fn() -> usize {  ...': fn() -> usize
            224..286 '{     let c = Cou...': usize
            234..235 'c': Counter
            238..245 'Counter': fn(struct { n: usize }) -> Counter
            238..263 'Counter(struct { ...': Counter
            246..262 'struct { n = 3 }': struct { n: usize }
            259..260 '3': usize
            269..270 'c': Counter
            269..275 'c.bump': fn(usize, Counter) -> Counter
            269..278 'c.bump(2)': Counter
            269..282 'c.bump(2).get': fn(Counter) -> usize
            269..284 'c.bump(2).get()': usize
            276..277 '2': usize
        "#]],
    );
}

#[test]
fn dot_call_happy_path_no_diagnostics() {
    check_diagnostics(
        r#"
type Counter = struct { n: usize } with {
    impl Self {
        get = fn(c: Self) -> usize { c.n };
    }
};
static main = fn() -> usize { Counter(struct { n = 3 }).get() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn dot_call_on_generic_type_member() {
    check_infer(
        r#"
type Box2 = struct::<T> { v: T } with {
    impl Self {
        get = fn(b: Self) -> T { let Box2(struct { v }) = b; v };
        remade = fn(b: Self) -> Self { let Box2(struct { v }) = b; Box2::<T>(struct { v }) };
    }
};
static main = fn() -> str {
    Box2(struct { v = "hi" }).remade().get()
};
"#,
        expect![[r#"
            240..300 'fn() -> str {    ...': fn() -> str
            252..300 '{     Box2(struct...': str
            258..262 'Box2': fn(struct { v: str }) -> Box2::<str>
            258..283 'Box2(struct { v =...': Box2::<str>
            258..290 'Box2(struct { v =...': fn(Box2::<str>) -> Box2::<str>
            258..292 'Box2(struct { v =...': Box2::<str>
            258..296 'Box2(struct { v =...': fn(Box2::<str>) -> str
            258..298 'Box2(struct { v =...': str
            263..282 'struct { v = "hi" }': struct { v: str }
            276..280 '"hi"': str
        "#]],
    );
}

#[test]
fn member_self_resolves_in_signature_and_body() {
    check_diagnostics(
        r#"
type Wrap = struct { v: usize } with {
    impl Self {
        dup = fn(w: Self) -> Self { Self(struct { v = w.v }) };
    }
};
static main = fn() -> Wrap { Wrap(struct { v = 1 }).dup() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn member_signature_must_be_fully_annotated() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        f = fn(a) { a.x };
    }
};
"#,
        expect![[r#"
            61..62: member `f` must spell its full signature: every parameter and the return type
            73..74: cannot determine the type of this expression; add a type annotation
        "#]],
    );
}

#[test]
fn statics_are_never_dot_callable() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static double = fn(a: A) -> usize { a.get() * 2 };
static main = fn() -> usize {
    let a = A(struct { x = 3 });
    a.double()
};
"#,
        expect![[r#"
            224..234: no field or member `double` on `A` (a module-level `double` is defined here — statics are never dot-callable; call `double(...)` instead at 113..119)
        "#]],
    );
}

#[test]
fn member_without_self_last_param_is_not_dot_callable() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        mk = fn(x: usize) -> usize { x };
        rev = fn(a: Self, x: usize) -> usize { x };
    }
};
static main = fn() -> usize {
    let a = A(struct { x = 3 });
    a.mk(1) + a.rev(2)
};
"#,
        expect![[r#"
            223..230: `mk` is not dot-callable: its last parameter is neither `Self` nor a safe borrow of `Self` (dot-call resolution is structural) (`mk` is defined here at 61..63)
            233..241: `rev` is not dot-callable: its last parameter is neither `Self` nor a safe borrow of `Self` (dot-call resolution is structural) (`rev` is defined here at 103..106)
        "#]],
    );
}

#[test]
fn member_reached_without_a_call() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static main = fn() -> usize { A(struct { x = 3 }).get };
"#,
        expect![[r#"
            136..159: `get` is a member fn, not a field; call it: `.get(...)`
        "#]],
    );
}

#[test]
fn duplicate_member_reported() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
        get = fn(a: Self) -> usize { 0 };
    }
};
"#,
        expect![[r#"
            105..108: duplicate member `get` (first defined here at 61..64)
        "#]],
    );
}

#[test]
fn member_may_share_a_field_name_getter_idiom() {
    // SEPARATE NAMESPACES (G13): a member named like a field is
    // legal. Call syntax selects the MEMBER (which may read the FIELD of
    // the same name through bare access in its own body) — the getter
    // idiom, clean end to end.
    check_infer(
        r#"
type Vecish = struct { len: usize } with {
    impl Self {
        len = fn(v: Self) -> usize { v.len };
    }
};
static main = fn() -> usize {
    let v = Vecish(struct { len = 3 });
    v.len() + v.len
};
"#,
        expect![[r#"
            129..206 'fn() -> usize {  ...': fn() -> usize
            143..206 '{     let v = Vec...': usize
            153..154 'v': Vecish
            157..163 'Vecish': fn(struct { len: usize }) -> Vecish
            157..183 'Vecish(struct { l...': Vecish
            164..182 'struct { len = 3 }': struct { len: usize }
            179..180 '3': usize
            189..190 'v': Vecish
            189..194 'v.len': fn(Vecish) -> usize
            189..196 'v.len()': usize
            189..204 'v.len() + v.len': usize
            199..200 'v': Vecish
            199..204 'v.len': usize
        "#]],
    );
}

#[test]
fn parenthesized_field_access_is_a_value_call_not_a_dot_call() {
    // `(recv.name)(...)` is an ordinary value call of the FIELD access —
    // the parens opt out of member selection, so a member-only name gets
    // the member-must-be-called diagnostic from the bare-access path.
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static main = fn() -> usize { (A(struct { x = 1 }).get)() };
"#,
        expect![[r#"
            137..160: `get` is a member fn, not a field; call it: `.get(...)`
        "#]],
    );
}

#[test]
fn calling_a_plain_field_names_the_field_and_the_missing_member() {
    check_diagnostics(
        r#"
type A = struct { x: usize };
static main = fn() -> usize { A(struct { x = 1 }).x() };
"#,
        expect![[r#"
            61..84: field `x` is not callable (its type is `usize`), and `A` has no member `x`
        "#]],
    );
}

#[test]
fn qualified_inherent_member_reference_is_live() {
    // G13: `Type::member(value)` names the type's OWN member — the
    // escape a collision's diagnostic points at, and legal on its own.
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static main = fn() -> usize { A::get(A(struct { x = 1 })) };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn dot_call_is_field_first() {
    // A fn-valued field keeps today's meaning on receivers without
    // members: `a.f(5)` on a record receiver calls the FIELD's fn value —
    // structural records cannot carry members, so the field is the only
    // namespace in play. (On NAMED receivers, call syntax selects a
    // member FIRST — see `member_may_share_a_field_name_getter_idiom`.)
    check_infer(
        r#"
static main = fn() -> usize {
    let a = struct { f = fn(n: usize) -> usize { n + n } };
    a.f(5)
};
"#,
        expect![[r#"
            15..103 'fn() -> usize {  ...': fn() -> usize
            29..103 '{     let a = str...': usize
            39..40 'a': struct { f: fn(usize) -> usize }
            43..89 'struct { f = fn(n...': struct { f: fn(usize) -> usize }
            56..87 'fn(n: usize) -> u...': fn(usize) -> usize
            59..60 'n': usize
            78..87 '{ n + n }': usize
            80..81 'n': usize
            80..85 'n + n': usize
            84..85 'n': usize
            95..96 'a': struct { f: fn(usize) -> usize }
            95..98 'a.f': fn(usize) -> usize
            95..101 'a.f(5)': usize
            99..100 '5': usize
        "#]],
    );
}

#[test]
fn dot_call_arity_does_not_count_self() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        add = fn(y: usize, a: Self) -> usize { a.x + y };
    }
};
static main = fn() -> usize { A(struct { x = 1 }).add() };
"#,
        expect![[r#"
            150..175: expected 1 argument(s), found 0
        "#]],
    );
}

#[test]
fn dot_call_argument_mismatch_blames_the_argument() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        add = fn(y: usize, a: Self) -> usize { a.x + y };
    }
};
static main = fn() -> usize { A(struct { x = 1 }).add("no") };
"#,
        expect![[r#"
            174..178: type mismatch: expected `usize`, found `str`
        "#]],
    );
}

#[test]
fn member_bodies_are_checked() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> str { a.x };
    }
};
"#,
        expect![[r#"
            88..91: type mismatch: expected `str`, found `usize` (expected `str` because of this return type at 79..85)
        "#]],
    );
}

#[test]
fn members_call_members_through_the_dot() {
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
        twice = fn(a: Self) -> usize { a.get() + a.get() };
    }
};
static main = fn() -> usize { A(struct { x = 2 }).twice() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn enum_types_can_carry_members() {
    check_diagnostics(
        r#"
type Light = enum { Red, Green } with {
    impl Self {
        flip = fn(l: Self) -> Light {
            match l {
                ::Red => Light::Green,
                ::Green => Light::Red,
            }
        };
    }
};
static main = fn() -> Light { Light::Red.flip() };
"#,
        expect![""],
    );
}

#[test]
fn reserved_groups_mint_no_members() {
    // A member inside a constrained group parses (and the group carries
    // its reservation diagnostic) but resolves nothing.
    check_diagnostics(
        r#"
type A = struct { x: usize } with T: copy {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static main = fn() -> usize { A(struct { x = 1 }).get() };
"#,
        expect![[r#"
            35..42: `with T: ...` constrained groups are not supported yet
            144..169: no field or member `get` on `A`
        "#]],
    );
}

#[test]
fn reserved_member_spelling_mints_no_member() {
    // A `type`-keyword member parses inside a plain `impl Self { ... }`
    // (and validation flags its spelling as reserved) but must not mint a
    // dot-callable item — a minted item here would make the "not
    // supported yet" diagnostic a lie, since the member would still
    // resolve and dot-call despite being reserved.
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        type get = fn(a: Self) -> usize { a.x };
    }
};
static main = fn() -> usize { A(struct { x = 1 }).get() };
"#,
        expect![[r#"
            61..65: associated types are not supported yet
            141..166: no field or member `get` on `A`
        "#]],
    );
}

#[test]
fn dot_call_on_type_without_members() {
    check_diagnostics(
        r#"
type A = struct { x: usize };
static main = fn() -> usize { A(struct { x = 1 }).get() };
"#,
        expect![[r#"
            61..86: no field or member `get` on `A`
        "#]],
    );
}

#[test]
fn member_signature_pinned_to_concrete_args_is_not_self_typed() {
    // On a generic type, `Self` means the type at its FULL binders — a
    // member whose last param pins the args is not dot-callable.
    check_diagnostics(
        r#"
type Box2 = struct::<T> { v: T } with {
    impl Self {
        get_pinned = fn(b: Box2::<usize>) -> usize { b.v };
    }
};
static main = fn() -> usize { Box2(struct { v = 1 }).get_pinned() };
"#,
        expect![[r#"
            156..191: `get_pinned` is not dot-callable: its last parameter is neither `Self` nor a safe borrow of `Self` (dot-call resolution is structural) (`get_pinned` is defined here at 65..75)
            174..175: cannot infer the type of this number: it has no defining use — add a type annotation
        "#]],
    );
}

/// The signature/body query split for MEMBERS: editing one member's
/// BODY re-infers only that member — its sibling's signature and the
/// dot-calling item both backdate (member signatures are annotation-
/// derived through the range-free `type_members`, which a body edit
/// leaves value-equal).
#[test]
fn firewall_member_body_edit_does_not_reinfer_siblings_or_callers() {
    use salsa::Setter as _;
    use std::sync::{Arc, Mutex};

    let log: Arc<Mutex<Vec<String>>> = Arc::default();
    let log_handle = Arc::clone(&log);
    let mut db = RootDatabase::with_event_callback(Box::new(move |event| {
        if let salsa::EventKind::WillExecute { database_key } = event.kind {
            log_handle.lock().unwrap().push(format!("{database_key:?}"));
        }
    }));

    let text_v1 = "type A = struct { x: usize } with {\n\
                       impl Self {\n\
                           get = fn(a: Self) -> usize { a.x };\n\
                           dbl = fn(a: Self) -> usize { a.x * 2 };\n\
                       }\n\
                   };\n\
                   static use_it: fn(A) -> usize = fn (a: A) -> usize { a.get() };\n";
    // Only `get`'s BODY changes; every signature stays identical.
    let text_v2 = "type A = struct { x: usize } with {\n\
                       impl Self {\n\
                           get = fn(a: Self) -> usize { a.x + 0 };\n\
                           dbl = fn(a: Self) -> usize { a.x * 2 };\n\
                       }\n\
                   };\n\
                   static use_it: fn(A) -> usize = fn (a: A) -> usize { a.get() };\n";

    let file = SourceFile::new(&db, "test.must".to_owned(), text_v1.to_owned());
    for item in crate::all_checkable_items(&db, file) {
        crate::infer::infer(&db, item);
    }
    let executed_infers = |log: &Mutex<Vec<String>>| {
        log.lock()
            .unwrap()
            .iter()
            .filter(|entry| entry.starts_with("infer("))
            .count()
    };
    // The type item itself has no body to infer, but its query still runs
    // once: 4 checkable units (A, get, dbl, use_it).
    assert_eq!(executed_infers(&log), 4, "all units inferred initially");

    log.lock().unwrap().clear();
    file.set_text(&mut db).to(text_v2.to_owned());
    for item in crate::all_checkable_items(&db, file) {
        crate::infer::infer(&db, item);
    }
    let log = log.lock().unwrap();
    assert_eq!(
        log.iter()
            .filter(|entry| entry.starts_with("infer("))
            .count(),
        1,
        "only the edited member may re-infer; executed: {log:#?}"
    );
}

#[test]
fn no_auto_deref_through_raw_pointers() {
    // NO auto-deref, ever (G14): a pointer to a type with members does not
    // dot-call them. A raw pointer is not a decayed borrow, so G14's
    // bounded exception — which reborrows a borrow receiver into a
    // borrow-`Self` member — never reaches it.
    check_diagnostics(
        r#"
type A = struct { x: usize } with {
    impl Self {
        get = fn(a: Self) -> usize { a.x };
    }
};
static main = fn() -> usize {
    let mut a = A(struct { x = 1 });
    let p = a.&raw mut;
    p.get()
};
"#,
        expect![[r#"
            203..206: no field `get` on `A.&raw mut`
        "#]],
    );
}

#[test]
fn generic_owner_member_annotations_are_clean() {
    // The annotation mirror must treat the OWNER's binder (and `Self`) as
    // bound inside member signatures and bodies — no spurious unknown-type
    // diagnostics.
    check_diagnostics(
        r#"
type Stack = struct::<T> { top: T, rest: usize } with {
    impl Self {
        peek = fn(s: Self) -> T { let Stack(struct { top, rest }) = s; top };
        rebuilt = fn(s: Self) -> Stack::<T> {
            let Stack(struct { top, rest }) = s;
            let keep: T = top;
            Stack::<T>(struct { top = keep, rest })
        };
        // The owner's `T` as a member PARAMETER annotation, and TWO
        // occurrences of it unifying with each other — both read once, so
        // neither needs a bound the owner's binder could not carry.
        echo = fn::<@a>(x: T, s: Self.&::<@a>) -> T { x };
        same = fn::<@a>(x: T, y: T, s: Self.&::<@a>) -> bool { x == y };
    }
};
static main = fn() -> usize { Stack(struct { top = 4, rest = 0 }).rebuilt().peek() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn const_generic_owner_member_annotations_are_clean() {
    check_diagnostics(
        r#"
type Buf = struct::<const N: usize> { used: usize } with {
    impl Self {
        cap = fn(b: Self) -> usize { N };
        pad = fn(b: Self) -> [usize; N] { [0; N] };
    }
};
static main = fn() -> usize { Buf::<8>(struct { used = 3 }).cap() };
"#,
        expect![[r#""#]],
    );
}

#[test]
fn expression_self_is_rigid() {
    // Expression-position `Self` is the RIGID Self (one meaning of `Self`
    // per body): on a generic owner it constructs at the member's own
    // binders — so a literal payload where `T` is expected is a mismatch,
    // and `let x: Self = Self(...)` round-trips clean.
    check_diagnostics(
        r#"
type Box2 = struct::<T> { v: T } with {
    impl Self {
        keep = fn(b: Self) -> Self {
            let Box2(struct { v }) = b;
            let x: Self = Self(struct { v });
            x
        };
        bad = fn::<@a>(b: Self.&::<@a>) -> Self { Self(struct { v = 1 }) };
    }
};
"#,
        expect![[r#"
            273..274: type mismatch: expected `T`, found `{number}` (expected `T` because of this field declaration at 27..31)
        "#]],
    );
}
