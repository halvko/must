= Must lang

Consistent function/closure syntax:
```must
static main: fn() -> () = fn() -> () {
    let s: str = "hello";
    (fn (s: str) -> () { print(s) })(s);
};
```
The `print(s)` above writes exactly what it is given — a `str`, verbatim,
with no newline appended — so a line break comes from the string, never
from the call.

or with less annotation (an unannotated parameter's type is still inferred,
from the single call site right here):
```must
static main = fn {
    let s = "hello";
    (fn (s) { print(s) })(s);
}
```
and ofc one could do bs like this — return the builtin `print` from a
nullary fn, then call the result with `s`, sidestepping the no-capture rule
entirely because nothing is ever captured:
```must
static main = fn {
    let s = "hello";
    (fn { print })()(s);
}
```

```must
static example = fn (arg: fn() -> usize) -> usize {
    arg()
}

static main = fn {
    example(fn () -> usize { 42 + 69 })
}
```

Plain structs: e.g. the type `struct { a: str }` is a valid type (without a
tag) — anonymous and structural, matched by shape alone. Enums have no
anonymous form: every enum lives behind a `type` item (see "Enums and
variants" below).

== String literals

Strings are multiline — a literal newline inside a string literal is legal — and
support six escapes: `\n \t \r \\ \" \0`. Anything else after a backslash is an
error anchored at the escape itself, and a trailing lone backslash is an
unterminated string. The lexer and the value decoder share one table, so they
can never disagree about what counts as a valid escape.

== Variable declarations

`let` is the only variable-declaration form. Possible sugars — an
irrefutable `match` statement, `if match`, `match ... else` — are not in
v1 (see "Match" below for the full not-yet list); a `type` item also can't
be declared locally inside a function body, only at the top level. Today you
write the `match` as an ordinary expression and bind its result with a
plain `let`:

```must
static normal_let = fn (s: str) -> str {
    let s2 = s;
    s2
}

type Maybe = enum { Some(str), None };

static from_match = fn (m: Maybe) -> str {
    let s = match m {
        Maybe::Some(s) => s,
        Maybe::None => "hello",
    };
    s
}
```

== Const functions and const blocks

Every item initializer — `static` and `const` alike — is evaluated at
compile time. The item keyword is about identity, not about when things
run: a `static` names one place that every use refers to, a `const` is
copied into each use.

Function const-ness is its own, explicit marker on the fn literal —
`const fn` — and is orthogonal to the item keyword:

```must
static double = const fn (n: usize) -> usize { n * 2 };
static fortnight = double(1209600); // ok: `double` is a `const fn`
```

The const context rules:

- An item initializer is a const context.
- The body of a plain `fn` literal is runtime code — entering it exits the
  const context, anything goes there. (This is why `static main = fn {
  print("hi") }` is fine: only the *definition* of the literal happens at
  compile time.)
- The body of a `const fn` literal is always a const context: it has to be
  const-evaluable wherever the literal ends up.
- `const { ... }` re-enters a const context wherever it appears — its point
  is getting back to compile time from inside runtime code.

In a const context a call is only allowed when the callee is *visibly* a
`const fn`: a name resolving to an item whose initializer is a `const fn`
literal, or a directly-called `const fn` literal. Const-ness is not part of
function types (yet), so a parameter or a let-bound value is rejected even
when it is provably bound to a `const fn` — conservative by design, and no
wrappers are peeled to find a fn literal behind an item's initializer:

```must
static double = const fn (n: usize) -> usize { n * 2 };
static f = fn {
    let d = double;
    const { d(2) }; // error: whether `d` is a `const fn` is not known
};
```

Const evaluation has no side effects, with one exception: `panic`. Calling
`print` in a const context is an error; calling `panic` is allowed —
failing loudly at compile time is the point of putting code there.

== Mutability

`let` bindings are immutable by default; `let mut` opts into assignment.
Assignment is a statement, not an expression:

```must
static count = fn (n: usize) -> usize {
    let mut total = 0;
    total = total + n;
    total
};
```

Fields follow their binding: when a record binding is `mut`, its fields
(and fields of fields, through named types too) are assignable — `p.x = 1;`
— and when it isn't, they are exactly as frozen as the binding itself.
Mutability is transitive from the binding, in both directions; there is no
per-field `mut`:

```must
static shift = fn () -> usize {
    let mut p = struct { x = 1, y = 2 };
    p.x = 10;       // fine: `p` is `mut`, so its fields are writable
    p.x + p.y
};
```

Parameters take `mut` the same way, before the name. A `mut` parameter is a
local copy — mutating it is invisible to the caller:

```must
static clamp_to_ten = fn (mut n: usize) -> usize {
    if n > 10 { n = 10; };
    n
};
```

Local mutation is allowed in const contexts. Const evaluation having no side
effects means no *observable* effects — mutating a binding in a private
frame that no one else can see is fine:

```must
static x = const { let mut n: usize = 1; n = n + 1; n };
```

`static mut` does not exist (deferred until there is a story for it): a
`static` names one place, but that place cannot be reassigned, a `const`
doesn't even have a single place an assignment could write to, and builtins
are not assignable either.

A rejected assignment is a check-time diagnostic, and — like every deferred
error — running code that reaches one crashes at exactly the place the
checker complained about, with the same message the squiggle showed.

== Statics and consts are accessible in their declarations

```must
static fib = fn (n: usize) -> usize {
    if n < 2 {
        n
    } else {
        fib(n - 1) + fib(n - 2)
    }
}

const fib2 = fn (n: usize) -> usize {
    if n < 2 {
        n
    } else {
        fib2(n - 1) + fib2(n - 2)
    }
}
```

== Statics and consts can be mutually recursive
```must
const fib1 = fn (n: usize) -> usize {
    if n < 2 {
        n
    } else {
        fib2(n - 1) + fib2(n - 2)
    }
}

static fib2 = fn (n: usize) -> usize {
    if n < 2 {
        n
    } else {
        fib1(n - 1) + fib1(n - 2)
    }
}
```

== functions can destructure arguments

A parameter is a *pattern* against a declared type, not just a name — its
own pattern language, distinct from what `match` arms accept: a name,
`mut name`, `_`, a named type's pattern peeling one layer of newtype, or a
record pattern (no variant patterns, and no sub-pattern below a bound
field). A record pattern binds fields by name, optionally renaming with
`as`, and a named type's pattern peels the newtype before the record
pattern underneath sees it:

```must
type Span = struct { lo: usize, hi: usize };

static width = fn (Span(struct { lo, hi }): Span) -> usize {
    hi - lo
};

static example = fn (struct { n, .. }: struct { n: usize, a: usize }) -> usize {
    n
};
```

A record pattern must account for every field — name it or end with
`..` — and a named field binds under its own name unless renamed
(`lo as l`); there is no nested field sub-pattern, so a bound field is
projected with `.` like any other value. A field may take `mut` of its
own (`struct { mut lo }`, before the field name even when the binding is
renamed): that is the *binding* opting into assignment, exactly as
`mut name` does, and says nothing about the record it was destructured
from.

Sketched, not implemented: a shorthand that folds the annotation into
the pattern (`fn ({n: usize})`), an opt-in `...` that allows — and
ignores — extra fields ("duck typing") on plain records and through a
named type's exposed fields, and field-level visibility (`pub(self)`,
`pub(mod)`) gating which fields a duck-typed pattern may see through a
named type.

== Named types

A `type` item declares a new *nominal* type over a structural record — a
newtype, not an alias. It is deliberately neither `static` (types have no
runtime location) nor `const` (reserved for future type aliases). A `type`
item's value is a `struct` literal or an `enum` literal (see the next
section).

```must
type Point = struct { x: usize, y: usize };
```

`Point` unifies only with itself: same declaration or nothing. There is no
implicit coercion between a named type and its underlying record shape, in
either direction — a `Point` is not a `struct { x: usize, y: usize }` and
vice versa (the error explains the underlying shape and how to construct).

Construction is a plain function call: the type name applied to the
underlying record value. Constructors are functions — no special brace
syntax on the type name.

Inside a record, `:` and `=` never mean the same thing: `:` annotates a
type and `=` defines a value. So `struct { x: usize }` is a shape,
`struct { x = 1 }` builds one, and `struct { x: usize = 1 }` spells both
halves at once. A field written bare is shorthand for defining it from a
binding of the same name — `struct { x }` means `struct { x = x }`.

```
static origin = Point(struct { x = 0, y = 0 });

static translate = fn (p: Point, dx: usize) -> Point {
    // Field access projects through to the declared shape.
    Point(struct { x = p.x + dx, y = p.y })
};
```

A bare `Point` in expression position is an error (`Point` is a type, not a
value); the construction head is its one legal expression position.
Construction is pure, so it is legal in const contexts.

At runtime named types are fully erased: a `Point` value *is* its record
value — same representation, structural equality under the hood. The type
system alone keeps `Point` and bare records apart, so erased equality is
only ever asked between two values of the same nominal type.

== Inherent members and dot-calls

A `type` declaration may carry operations of its own, in a trailing `with`
chain. Inside it, `impl Self { ... }` holds *inherent members*: ordinary
`fn` values, defined with `=` like every other item, that the type owns.

```must
type Counter = struct { n: usize } with {
    impl Self {
        get = fn (c: Self) -> usize { c.n };
        bump = fn (by: usize, c: Self) -> Self {
            Counter(struct { n = c.n + by })
        };
    }
};

static main = fn () -> usize {
    let c = Counter(struct { n = 3 });
    c.bump(2).get()
};
```

There is no `self` keyword. Dot-callability is *structural*: a member is
reachable through the dot exactly when its LAST parameter is `Self`-typed,
and the receiver becomes that last argument. `c.bump(2)` means `bump(2, c)`
— literally, including evaluation order, so the written arguments run
*before* the receiver expression binds. That is the whole reason the
receiver sits last rather than first.

A member must spell its full signature: every parameter type and the return
type. That is what lets a dot-call pick the member without first running
inference over its body.

Module-level functions are deliberately *not* dot-callable, even with the
same shape — construction has no receiver, so `Counter::new`-style
constructors stay ordinary calls:

```must
static counter_new = fn (n: usize) -> Counter { Counter(struct { n }) };
// counter_new(3)      — fine
// c.counter_new()     — error: no field or member `counter_new`
```

Fields and members are separate namespaces, and the syntax picks between
them: call syntax reaches a dot-callable member first, a bare dot always
reads the field, and parenthesising the access — `(v.len)()` — calls a
fn-typed field even when a member shares its name. A member named after a
field is therefore legal, which is what makes the getter idiom work:

```must
type Vecish = struct { len: usize } with {
    impl Self {
        len = fn (v: Self) -> usize { v.len };
    }
};
// v.len()  — the member
// v.len    — the field
```

There is no auto-deref and no auto-ref. A receiver's type must *be* the
member's `Self`, so a raw pointer to a type with members does not dot-call
them; write the deref yourself.

The owner's generic binder is in scope in member signatures and bodies, and
a dot-call never spells a turbofish — the receiver's type supplies the
arguments:

```must
type Box2 = struct::<T> { v: T } with {
    impl Self {
        get = fn (b: Self) -> T { b.v };
    }
};

static hi = fn () -> str { Box2(struct { v = "hi" }).get() };
```

Attaching an impl to anything but `Self` is a *trait impl* — its own
section, next.

== Traits

A `trait` item declares a set of *requirements*: named, fully-signatured
`fn`s an implementer must supply. A trait is a bound, never a type — it
cannot be named as a value or written where a type is expected.

```
trait Write = requires {
    push: fn(s: str, w: Self) -> Self;
};
```

An impl lives in a `with`-chain, exactly like an inherent member, but its
element is headed by the trait's name instead of `Self`. There are two
homes for it. A *type-side* impl sits in the implementing type's own
chain, headed by the trait:

```
type Sink = struct { pushes: usize } with {
    impl Write {
        push = fn(s: str, w: Self) -> Self {
            print(s);
            Sink(struct { pushes = w.pushes + 1 })
        };
    }
};
```

A *trait-side* impl sits in the trait's own chain, headed by the
implementing type — the only way to implement a trait for a builtin
scalar, which has no declaration of its own to host a chain:

```
trait Display = requires {
    fmt: fn::<W: Write>(w: W, x: Self) -> W;
} with {
    impl usize {
        fmt = fn::<W: Write>(w: W, x: usize) -> W { w.push("n") };
    }
};
```

At most one impl per (trait, type) pair, whichever home it sits in — a
second impl, in either home, is an error naming both sites. A requirement's
binder may carry bounds of its own, composed with `+` like any other bound
list — `Display`'s `fmt` is generic over its sink, `W: Write`, independent
of the trait's own (here absent) generic parameters. A requirement declares
a signature and no body, so each of its parameters is a plain `name: Type`:
`mut` and destructuring patterns are refused there and belong to the impl
that supplies the body.

Inside a generic body, a bound re-opens exactly the bounded trait's
members on the rigid receiver — an ordinary structural dot-call, "bound-
directed" only in *how* the member is found, not in its calling
convention:

```
static show = fn::<T: Display>(x: T) -> usize {
    let s = Sink(struct { pushes = 0 });
    let s = x.fmt(s);
    s.pushes
};
```

Outside a bound, a trait member is reached through the *qualified short
form*, `Trait::member(args)`, with `Self` inferred from the arguments —
`Display::fmt(w, p.x)` picks the `usize` impl because `p.x` is a `usize`:

```
type Point = struct { x: usize, y: usize } with {
    impl Display {
        fmt = fn::<W: Write>(w: W, p: Self) -> W {
            let w = Display::fmt(w, p.x);
            w.push(",")
        };
    }
};

static main = fn() -> usize {
    show::<usize>(7) + show::<Point>(Point(struct { x = 1, y = 2 }))
};
```

Dispatch is static only: every call above resolves to one impl at compile
time, passed along as a dictionary of fn values under the hood. There is
no `dyn` and no vtable.

A dot-call whose name is ambiguous on the receiver's type — a field and a
member of the same name, or same-named members from more than one trait —
is refused at the call; the spellings that pick one are `Type::member(value)`
for an inherent member, `Trait::member(value)` for a trait's,
`Trait::<Self = Type>::member(value)` when the trait's short form itself
needs the implementing type spelled out, and `(value.field)(...)` to call
a fn-typed field.

A trait declaration is non-generic today, and so is every implementing
type — reserved for later: trait aliases (`trait Ord = Eq + PartialOrd`),
generic traits and generic-type impls, supertrait clauses, default
members, associated types and consts, `unsafe` traits and trait members,
and marker impls (`impl Name;`). Also reserved: using a bounded generic
`fn` as a value; and using a bound from inside a `fn` literal or a
`const { ... }` block nested in the bounded body (the dictionary lives in
the enclosing body, out of a nested one's reach). Bounds on a `type`
declaration's own binder are reserved too — only a member's own binder,
or a generic `fn`'s, can carry them today.

== Enums and variants

A `type` item's value may also be an `enum` literal. Variants carry zero or
more *positional* payload types (named-field payloads come later; a payload
may be a `struct { ... }` record type, which covers the same ground).

```
type Shape = enum { Circle(usize), Pair(usize, str), Point };
```

*Variant types are first-class.* `Shape::Circle(3)` has type
`Shape::Circle` — not `Shape`. The precise type survives as long as
possible: annotations may demand a specific variant (`fn (c:
Shape::Circle)`), and an API can force callers to prove which state they
are in. A payload-less variant path (`Shape::Point`) *is* the value; a
variant with payloads is a constructor function `fn(usize) ->
Shape::Circle`, usable first-class (`let make = Shape::Circle;
make(3)`). Constructors are pure, so construction is legal in const
contexts.

*Widening is a conversion, not subtyping.* A variant-typed value is
**tag-free** at runtime: `Shape::Circle(3)` is just the payload — for code
that stays on one variant (a state machine whose states are variants) the
enum is fully erased, exactly like named struct types. An *enum-typed*
value is **tagged**: which variant it is, plus the payload. Going from
`Shape::Circle` to `Shape` therefore does real runtime work (injecting the
tag), so it is an implicit *conversion* applied where a context demands
the enum — an annotation, a call argument, a return type — never an
equality the inference engine can run backwards. The conversion is
shallow: `fn() -> Shape::Circle` is not a `fn() -> Shape`.

`if`/`else` values join family-aware: leaves that agree on one variant
keep that variant (full precision, zero conversions); mixed variants of
one enum widen to the enum, with the conversion at each branch edge;
variants of *different* enums are incompatible branches, as ever.

*`let mut` widening.* An unannotated `let mut` initialized with a
variant-typed value widens the binding to the enum at binding time — a
mutable state variable is meant to be reassigned across variants:

```
static run = fn {
    let p = Shape::Point;          // p: Shape::Point  (plain let keeps precision)
    let mut s = Shape::Point;      // s: Shape         (mut widens at binding)
    s = Shape::Circle(1);          // fine: converts at the assignment
};
```

An annotated `let mut s: Shape::Circle = ...` keeps the annotation's
precision — the axiom always wins.

Bare `Shape` remains a type, not a value, and `Shape(...)` does not
construct (an enum has no single shape) — construction always goes through
a variant. Deconstruction goes through `match` (next section).

== Match

The basic braced form. Patterns are deliberately flat in v1 and mirror
construction: a variant pattern `::Variant(bindings...)` with positional
binding names (or `_` holes), the payload-less `::Variant`, `_`, and a
plain binding name that binds the whole scrutinee. A variant is written
with a leading `::` sigil — "a variant of the scrutinee's enum, enum
elided" — or fully qualified `Shape::Variant`; the two name the same thing
(the qualified spelling is construction's mirror image), and the sigil
resolves *type-directed* against the scrutinee's enum:

```
type Shape = enum { Circle(usize), Pair(usize, str), Point };

static describe = fn (s: Shape) -> str {
    match s {
        ::Circle(r) => "circle",
        Shape::Pair(n, text) => text,
        ::Point => "point",
    }
};
```

*The sigil mirrors into expression position too.* `::Circle(3)` and
`::Point` build a variant of whatever enum the position *expects*, exactly
as the pattern names a variant of whatever enum the scrutinee *has*:

```
static origin: Shape = ::Point;
static make = fn (r: usize) -> Shape { ::Circle(r) };
static describe_point = fn () -> str { describe(::Point) };
```

It reads the expected type and nothing else — it never runs inference
backwards to find one. So it works wherever the position already has a
type: an annotation, a return type, a call argument, an annotated `let`,
a record-literal field, an array element, an assignment's right-hand
side, a returned value, and the right operand of `==`/`!=`. Where
nothing pins the position — an unannotated `let`, and today also a
`match` arm's body or an `if` branch, whose types are decided by the
join *after* the branches are checked — it is a plain error naming the
qualified spelling. The qualified form is always available and stays the
canonical one; the sigil only ever removes a rejection, so nothing is
expressible with it that is not expressible without it.

A bare name in pattern position — no `::`, no qualifying enum — *always*
binds the whole scrutinee, even when it happens to spell a variant's name;
matching a variant requires the `::` sigil. This is deliberate: silent
type-directed reinterpretation of a bare name was a footgun — rename or
remove a variant later and an old bare-name arm would quietly degrade into
a catch-all with no error. A bare name that shadows a variant of the
scrutinee's enum is well-typed but warns, pointing at the `::Name`
spelling. Writing the retired `Name(...)` shape (a bare name with parens)
is a check-time error asking for `::Name(...)`.

Arms are branches of one join, exactly like `if`/`else`: arms that agree on
one variant keep that variant's precision, mixed variants of one enum widen
to the enum (the conversion sits on each arm's edge), and a match nested in
an `if`'s branch tail contributes its arms to the enclosing join — blame
speaks about the leaves.

*Exhaustiveness* is flat set-cover over the enum's variants: a `_` or
binding arm covers everything, and a `match` that misses variants is a
check-time error on the `match` keyword naming each uncovered variant.
Like every deferred error, running code that actually reaches an uncovered
variant crashes with exactly the squiggle's message. Arms that can never
run (after a catch-all, a variant already covered) are warnings, not
errors. On a non-enum scrutinee only `_` or a binding can match (for now).

*Matching a variant-typed scrutinee needs no dispatch at all.* A `fn (s:
State::Running)` knows statically which variant it holds, so a `match`
inside compiles to a direct payload destructure — no switch, no tag read,
the enum fully erased. That is the state-machine payoff: code that stays
on one variant pays nothing for the enum it belongs to. Arms naming the
*other* variants of the enum are legal but flagged unreachable.

`match` is pure control flow, so it is const-legal — fine inside `const
fn` bodies and `const { ... }` blocks.

Not in v1 (landing later as one coherent pattern-language feature): nested
patterns, or-patterns (`0 | 1`), guards, literal patterns, `if match`,
`match ... else`, the statement form `match x => pat;`, and record
patterns — though `..` is already reserved in pattern position for them.

== Loops

`loop { ... }` is an expression: an infinite loop. `break;` and `break
value;` exit it, `continue;` restarts the body. The loop's value is carried
by its breaks — the break values are branches of one join, exactly like
`if`/`else` branches and `match` arms, so they must agree on one type (and
family-aware variant widening applies to them too). The body's own tail
value is discarded: running off the body's end just continues the loop.

```
static sum_to_ten = fn () -> usize {
    let mut acc = 0;
    let mut i = 0;
    loop {
        if i == 10 { break acc; };
        acc = acc + i;
        i = i + 1;
    }
};
```

A bare `break;` carries `()`. A loop no value-carrying break ever exits
never finishes, so it types as the never type `!` (which widens into any
context, like `panic`). `break` and `continue` are themselves expressions
of type `!` — that is why `if i == 10 { break acc; }` needs no special
casing.

`break`/`continue` outside a loop is an error. Function literals bound the
loop context like they bound everything else: a `break` inside a `fn`
nested in a loop body does not exit the outer loop — it is the same error.
`const { ... }` blocks are compile-time units of their own and bound it
too.

Labels don't exist yet: `break` and `continue` always target the innermost
loop.

Loops are const-legal — fine in `const fn` bodies and `const { ... }`
blocks. Compile-time evaluation is fuel-bounded, so an infinite loop in an
initializer is not a hung compiler but the ordinary
"constant evaluation ran out of fuel" diagnostic at check time.

== Early return

`return value` leaves the enclosing function with that value; a bare
`return` leaves it with `()`. It is `break`'s sibling one tier up — an
*expression* of type `!`, not a statement form — so it composes wherever a
value goes and needs no special casing anywhere:

```must
static classify = fn (n: usize) -> str {
    if n == 0 { return "zero"; };
    let parity = if n == 1 { "one" } else { return "many" };
    parity
};
```

The value is checked against the enclosing function's return type — the
same check its tail expression gets. Where that type is written, a
mismatch blames the returned value and cites the annotation; where it is
being inferred, `return e` pins it exactly as a tail expression would, so
an un-annotated `fn (c: bool) { if c { return "yes"; }; "no" }` is
`fn(bool) -> str`.

A body that never completes at all pins nothing, so an un-annotated one
gets `!`: `fn { panic("boom") }` and `fn { loop { } }` are both
`fn() -> !`, callable from anywhere with no annotation to write and
usable wherever `!` widens. This is a default: a written slot the literal
is checked against (an annotation, an outer `let`/`static` type) gets
first say, so `let f: fn() -> usize = fn { panic(..) }` is
`fn() -> usize`, with the body's `!` coercing at the tail — only when
nothing pins it does the body's own divergence become the answer. An item
with no such slot of its own still concludes `!`, and that conclusion is
exactly as fixed as any other return type: `fn() -> T` is invariant (no
subtyping anywhere), so nothing widens it later on someone else's
say-so.

`return` targets the *nearest enclosing function literal*, and function
literals bound it the way they bound everything else: a `return` inside a
`fn` nested in another function returns from the inner literal, leaving
the outer one running. A `return` with no enclosing function at all (an
item initializer's own top level, which is a value expression rather than
a function) is an error, exactly like a `break` with no enclosing loop.

Inside a `const { ... }` block, `return` is *not supported yet*. What it
ought to mean is leaving the enclosing function — but a `const` block is
compiled as a body of its own, so carrying an exit across that boundary
needs machinery that does not exist yet, and the other reading (yielding
the block's value, the way a `const` block does bound `break`) would be a
different construct wearing the same spelling. So it is refused outright,
with a message saying exactly that. A `fn` literal *inside* a `const`
block is its own body, so a `return` in that one is ordinary and legal.

`return` is const-legal otherwise: a `const fn` may exit early.

Statements after a `return` still type-check and still resolve — there is
no unreachable-code lint yet.

== Generics

A generic item binds type and const params on the fn literal itself —
`fn::<T, const N: usize>(...)` — and is instantiated with a turbofish
`::<...>`. Generic items are *firewall items*: never joined into an
inference group, so a body is checked exactly once, before any call site is
known, and every instantiation shares that one check. A type param `T` is
*rigid* inside the body — it unifies only with itself, never widening into
`usize` or anything else — which is what makes the single pre-instantiation
check sound.

```
// `const fn` here because `four`'s initializer, like every item initializer,
// is a const context, and a plain `fn` cannot be called from one.
static id = const fn::<T>(x: T) -> T { x };      // T inferred from the argument
static four = id::<usize>(4);                    // or pinned by turbofish
static square = const fn::<const N: usize>() -> usize { N * N };
static forty_nine = square::<7>();
```

A *const argument* is deliberately restricted grammar — it is one of three
things, and nothing else:

- a literal (`square::<7>`),
- a bare `const name`, forwarding a const param (`rep::<const N>` inside a
  generic body passes the binder's own `N` straight through), or
- a `const { ... }` block for anything compound (`square::<const { 3 + 4 }>`).

There is no additive-operator escape and no parenthesis escape: `const N + 1`
and `const (a > b)` no longer parse — the parser points at the braced
spelling, `const { ... }`. A bare `{ ... }` without the `const` keyword is
likewise a parse error asking for `const { ... }`. Const arguments are never
inferred from an ordinary call and never peeled from a runtime value; write
them explicitly at the turbofish.

A binder names each parameter once, across kinds: `fn::<T, T>` and
`fn::<T, const T: usize>` are both a *duplicate generic parameter* error,
reported at the second occurrence.

== Generic type declarations

A `type` item may bind its own type and const params, with
`struct::<...>` / `enum::<...>`, and reuse them across its fields or variant
payloads:

```
type Pair = struct::<T> { a: T, b: T };
type Option = enum::<T> { Some(T), None };
type Buf = struct::<const N: usize> { data: [usize; N], len: usize };
```

A generic type is instantiated with a turbofish in both annotation and
construction position. Construction is the same plain call as any named type
— the head applied to the underlying record — and the type argument may be
inferred from the payload, or escaped with `_`:

```
static labeled = Pair::<str>(struct { a = "left", b = "right" });
static inferred = Pair(struct { a = "x", b = "y" }); // T = str, from the payload
```

Const params make instances *distinct types*: `Buf::<8>` and `Buf::<9>` do
not unify (the const argument is checked, not decorative), and the length
flows into the field type — a `Buf::<8>` really does carry a `[usize; 8]`.
Variants of a generic enum spell their arguments before the variant name:
`Option::<usize>::Some(3)`, and mixed variants widen to the generic enum
exactly as in the non-generic case.

Honest restrictions, all diagnosed:

- A *bare* generic name in type position is an arity error (`Pair` takes 1
  generic argument, found 0) — a generic type's arguments are never optional
  in an annotation.
- A `const { ... }` block *cannot parameterize a type*: the argument to
  `Buf::<...>` must be a literal or a `const name`; a compound length is
  passed through a generic function's const parameter instead.
- A const param cannot be used as a *type* (`{ len: N }` is "`N` is a const
  parameter, not a type").
- A generic enum's *variant types* are not spellable in annotations yet
  (`fn (o: Option::Some)` is rejected with that reason); use the whole
  generic enum, or a monomorphic enum, until it lands.

== Raw pointers and unsafe

Raw pointers reach places all the way down. The types are `T.&raw`
(shared) and `T.&raw mut` (mutable). An address is taken with `place.&raw` /
`place.&raw mut`, and a place is now the full grammar: a variable, a chain
of its fields and elements (`r.a`, `a[i]`, `a[i][j]`), a `static` or `const`
item (a `const` use's own copy), or a chain rooted in a deref (`p.*.x` — a
place reached *through* a pointer). There is no address-of a temporary. A
pointer is followed with the postfix deref `p.*`; there is **no
auto-deref**, so `p.*` is the only way a pointer is ever read or written.
The old prefix spelling (`&raw place` / `&raw mut place`) is retired:
writing it gets a targeted migration diagnostic pointing at the postfix
form, not a silent reinterpretation. Plain `.&` / `.&mut`, with no `raw`,
are the SAFE borrows — see "Safe borrows and regions" below. Everything
else in this chapter is about the raw flavor.

```must
static main = fn () -> usize {
    let mut x = 1;
    let p = x.&raw mut;
    unsafe { p.* = 42; };
    x                       // 42
};
```

Forming a pointer is safe; *following* one is not. Every deref — read or
write — must sit inside an `unsafe { ... }` block, a lexical checker region
within a function; the message otherwise is `dereferencing a raw pointer
requires an `unsafe { ... }` block`. The deref rule covers the new places
evenhandedly: an assignment target like `p.*.x` *is* a deref write, and so
is the `p.*` inside `p.*.x.&raw mut` — computing an address through a
pointer follows one first. `place.&raw mut` additionally requires the root
binding to be `mut` — the existing transitivity rule verbatim — and blames
the root with the same "make it `mut`" quick fix an assignment would;
`place.&raw` (shared) needs no `mut`. `.&raw mut` of a `static` is rejected:
`static mut` stays deferred.

Writes through pointers are judged on the pointer, never on a binding. The
target of an assignment may be any deref-rooted chain — `p.* = v;`,
`p.*.x = v;`, even `p.*.xs[i] = v;` — and the judgment is the GOVERNING
pointer: the receiver of the chain's outermost deref must be `T.&raw mut`.
Writing through a shared `T.&raw` is rejected (`cannot assign through
`T.&raw`: writing needs a `.&raw mut` pointer`), and so is `p.*.x.&raw mut`
through a shared pointer — minting a mutating address must not launder the
shared flavor into a write permission. `p` itself never needs to be a `mut`
binding — writing through it reassigns nothing — and derefs deeper in the
chain are ordinary *reads* of raw pointers, which copy the pointer, and a
copy carries its whole permission, so their flavors don't matter. A
deref-rooted address is the original allocation's address with the path
extended — `p.*.x.&raw mut` hands out the very place `p` points to, one
field in; no copy is materialized on the way.

`x.&raw` on a local is the address of that frame slot, for that frame's
lifetime — and no longer. Nothing here promises stable addresses for locals
beyond their frame's life, nor address preservation across copies: copying a
value copies the pointer bits, never the pointee's identity. Return an
`x.&raw mut` past the frame that owns `x` and a later deref is a dangling
pointer. The interpreter *detects* the misuse cases and traps
deterministically. Address-taking itself never bounds-checks —
`a[i + 1].&raw mut` mints silently even past the end — so validity is
judged where the pointer is *used*: every deref first checks liveness and
the pointer's stored path, and a write through one mutates the pointee in
place, so pointers into it survive the write, exactly like real memory. The
traps: a deref after the owning frame returned
(`error[UndefinedBehavior]: dangling pointer — the local it pointed to no
longer exists (its frame has returned)`), a deref of an address minted out
of bounds (`error[UndefinedBehavior]: out-of-bounds pointer — it points to
element 5 of an array with 2 elements`), a write into read-only memory
(`error[UndefinedBehavior]: write through a pointer into read-only memory
(a `static`)`). But that detection is interpreter quality, not a language
guarantee: the program is undefined behavior, and a later backend may do
anything with it. Detected-UB traps make the interpreter a good teacher;
they do not make the code correct. A pointer value never renders as a
number either — `&raw <opaque>`, a fixed debug spelling, not source syntax.

Unsafe is legal in const contexts — pointers may be used freely during
compile-time evaluation — but a pointer can never *leave* it: a memoized
initializer whose value contains a pointer is `a pointer cannot leave
compile-time evaluation`. Statics and consts differ through a pointer just
as they do everywhere else, now *observably*: a `static` names one place, so
every `S.&raw` is the same address (`S.&raw == S.&raw` is `true`) — that
half is a language promise. A `const` is copied into each use, and the
interpreter happens to give every `C.&raw` its own temporary (`C.&raw ==
C.&raw` is `false`), but const-mention identity is deliberately
unspecified: a compiler may merge or split immutable copies. Compare
static-derived addresses; never const-derived ones.

Still reserved: `unsafe fn`.

== Heap allocation

The heap is built out of raw pointers, not a new kind of value: six
builtins, and everything above them — containers, arenas, growth — is
ordinary Must code (`examples/heap.must` is that library, twice over: a
growable vector and a typed arena).

- `alloc_array::<T>(n)` is safe and result-shaped. It returns
  `AllocResult::<T>`, a compiler-provided `enum { Ok(T.&raw mut), Err }`,
  so every allocation site says what it does when memory runs out (the
  interpreter's own allocator never answers `Err`; allocators written over
  it do). Fresh elements are *uninitialized*: reading one before its first
  write is detected UB, not a zero. `alloc_array(0)` is a trap — a refused
  request, defined behaviour.
- `dealloc_array(p, n)` is `unsafe` and exact-match: `p` is the head
  pointer `alloc_array` returned and `n` its element count. A non-head
  pointer, a wrong count, a local or a `static`, and a double free are
  each detected UB, with an "allocated here" note at the allocation's
  birth site. Not freeing is a leak, and a leak is not UB.
- `add(p, i)` is `unsafe` and takes a `usize`. Minting the address is
  unchecked, like `a[i].&raw mut` — an out-of-range result derefs to
  detected UB — but advancing a pointer that does not address an array
  element, with `i > 0`, is detected UB at the call itself, which is why
  it needs `unsafe` before any deref.
- `offset(p, i)` is `add`'s signed sibling: `unsafe`, takes an `isize`,
  and moves either direction. Minting past the end stays unchecked, like
  `add`; a result before the allocation's start (index `< 0`) is detected
  UB at the call, since no address before element 0 exists to mint.
- `copy(src, dst, n)` is `unsafe`, counts elements, and is memmove-shaped:
  overlapping ranges are defined, uninitialized elements copy silently,
  out of range on either side is detected UB, and a zero-length copy is
  valid through any pointer.
- `dangling::<T>()` is safe: a `T.&raw mut` that was never valid, the
  stand-in for "no buffer yet" (there is no null). Any deref is detected
  UB.

There is no `realloc`: growth is alloc, copy, dealloc, composed by the
container. Allocation never happens at compile time: under a const context
`alloc_array` and `dealloc_array` are refused eagerly; `add`, `offset`,
`copy` and `dangling` allocate nothing, so they are const-legal wherever
the values they touch already are.

```must
static main = fn () -> () {
    match alloc_array::<usize>(2) {
        AllocResult::Ok(p) => {
            let q = unsafe { add(p, 1) };
            unsafe {
                p.* = 1;
                q.* = 2;
            };
            unsafe { dealloc_array(p, 2); };
        }
        AllocResult::Err => print("out of memory"),
    };
};
```

== Safe borrows and regions

A borrow is a checked pointer. `x.&` borrows a place for reading, `x.&mut`
borrows it exclusively, and `r.*` reads through either — the same postfix
deref a raw pointer uses. What makes them different is that a borrow needs
no `unsafe`: safety is decided by the pointer's *flavor*, and `.&raw` is the
unsafe flavor. There is no auto-ref and no auto-deref, ever.

Every borrow says how long it is good for. That is its *region*, spelled
`@a`, and it rides the borrow operator's own turbofish:

```must
static get = fn::<@a>(r: usize.&::<@a>) -> usize {
    r.*
};
```

`@a` is a parameter of `get`, exactly as `T` would be, and it rides the same
binder list — `fn::<@a, T, const N: usize>` declares one of each. Regions
are nonetheless a *distinguished* kind: they are erased before anything is
compiled. Two functions whose signatures differ only in their regions lower
to identical code, no region reaches a monomorphization key, and no backend
ever sees one. A region can therefore reject a program and can never change
what it does.

Nothing is elided. Every region in a signature is written by hand, on
purpose, until enough real code exists to say which elision rule would have
earned its keep — so `usize.&` on its own is an error naming the spelling
rather than a guess. Inside a *body* the situation is different: there the
regions are inference variables, not parameters, so `@_` says "there is a
region here, work it out" and an omitted turbofish on a borrow expression
means the same. `@_` in a signature is rejected: a caller has to be able to
name what they are choosing. A binder is where that naming happens, so `@_`
is rejected there too — it asks for a region, it does not declare one.

Two regions can be joined. `usize.&::<@a + @b>` is a borrow good for as long
as *both* last — the largest region every listed region outlives — so `+`
reads as conjunction here exactly as it does in bound composition. Which
side of an obligation the join sits on decides what it costs: a borrow good
for `@a + @b` being used somewhere shorter needs *both* members to outlive
that place, while covering the join itself needs only *one* member covered,
because covering either already covers their overlap.

A branch construct forms a join without being written as one. Each branch
reborrows into the result, so `if c { p } else { q }` over two unrelated
regions is fine and its type is good for as long as both branches are — no
relation between `@a` and `@b` is demanded, and a `&mut` branch degrades
into a `&` context exactly as it would at a direct call. `match` arms, a
`loop`'s `break` values and an array literal's elements are the same join
through other spellings, and cost the same.

One consequence worth knowing, because it is the same limitation twice. A
value whose type comes from a join is not resolved until the end of the
statement, so *projecting straight off it* — `.*`, `.field` or `[i]` — has
nothing to project through yet, and reports that the type cannot be
determined. Annotating fixes it, and that is the general answer.

Borrows meet this in two places. An array literal whose elements are borrows
is a join like any other, so `[p, q][0].*` needs the array annotated
(`let arr: [usize.&::<@a>; 2] = [p, q];`). And an aggregate produced by a
branch — `(if c { struct { x = p } } else { … }).x` — needs the same, or a
function that takes the aggregate and does the projection inside. The
limitation predates borrows and is not specific to them; borrows are simply
what makes it common, because `.*` is the primary thing one does with a
borrow.

=== Exclusivity, and reborrowing at every use

A `.&mut` is *affine*: it cannot be duplicated, because duplicating it would
duplicate a permission. What repeats is projection *through* the place
holding it. A mention of a borrow-typed place in a position that *wants* a
borrow — an argument, an annotated binding, a return — mints a fresh,
shorter reborrow with the parent suspended for exactly that reborrow's span,
so a borrow can be passed to a function and then used again:

```must
static bump = fn::<@a>(m: usize.&mut::<@a>) -> () { m.* = m.* + 1; };

static bump_twice = fn::<@a>(m: usize.&mut::<@a>) -> usize {
    bump(m);
    bump(m);
    m.*
};
```

A language that moved on the first use would reject that. Reborrowing is
also what lets an exclusive borrow be used where a shared one is wanted:
that is not subtyping (a callee could stash a shared reference for the whole
of the parent's region) but a *shared reborrow* — the region shrinks and the
parent suspends. Nothing is written at the use site; the compiler inserts
it. That insertion is the single licensed exception to "no auto-ref, ever",
and it is bounded exactly: the compiler may insert a *safe* borrow of `x.*`
where `x` is already a borrow, never a borrow of `x` itself, and never a raw
borrow.

`.&mut` follows the same transitive-mutability rule assignments do — the
root binding must be `mut` — and no write permission may be reached through
a shared step, which is what stops a shared borrow laundering into one. That
is judged over the whole place rather than its outermost step: writing
through a `.&` is rejected (`cannot assign through `T.&`: writing needs a
`.&mut` borrow`), and so is writing through a `.&mut` that is itself held
behind a `.&`, because reading a `.&mut` out of a place *reborrows* it, and
a shared place may grant no such reborrow. Minting `.&mut` or `.&raw mut`
anywhere along such a chain is rejected the same way, and so is the implicit
reborrow, which is that mint with the `.&mut` left unwritten. A `.&raw mut`
in the middle stops the walk instead: reading a raw pointer out copies it,
and a copy carries its whole permission — the raw world's own laundering,
gated by `unsafe`. Neither safe flavor can be minted through a *raw* pointer
(`p.*.&`, `p.*.&mut`) — a raw pointer carries no region, so a safe borrow
minted through one would have nothing to bound it.

Reading `r.*` copies the referent when the referent is copyable. When it is
not, the read is rejected with `cannot move out of a borrow` — a rule that
matters because a `.&mut` is itself affine. It is a rule about `r.*` as a
*value*: a `.*` in place position — `r.*.x`, `r.*[i]`, `r.*.x = 9`,
`r.*.x.&mut`, and `r.*.*` where the referent is itself a borrow — projects
through the borrow instead of copying it, so an affine referent is no
obstacle there. Nor is `r.*` handed to a parameter that wants a borrow: that
is the reborrow above, which suspends the parent rather than copying it.

=== Outlives clauses

Returning a borrow at a region the caller chose is a promise that the value
lives that long. When one region has to cover another, the signature says
so:

```must
static longer = fn::<@a: @b, @b>(x: usize.&::<@a>) -> usize.&::<@b> {
    x
};
```

`@a: @b` reads "`@a` outlives `@b`", and the relation is transitive: with
`@a: @b` and `@b: @c` declared, `@a: @c` needs no restating. Without the
clause the same body is rejected, because only the caller knows which of two
region parameters is longer — the compiler cannot pick, and the signature is
where they are told. Borrowing a local and letting it escape the body is
rejected for the same reason, with `borrowed value does not live long
enough`.

=== What is checked, and what is checked *yet*

The shipped checker is the outlives module: it collects each body's outlives
obligations, solves for every region's value, and rejects the two things
that stage can see — an undeclared relation between signature regions, and a
borrow of a local that escapes. It is a pure per-body query whose entire
output is diagnostics; nothing it computes is consumed by lowering, layout
or selection.

What it does not yet do is *exclusivity*: deciding statically which borrows
may be live at once. Until that lands, the interpreter detects violations
dynamically — writing through a borrow that was invalidated by a later
borrow of the same place, using a borrow whose parent has been written
through, or touching a borrowed local by its own name while a borrow of it
is live — reported as undefined behavior where it happens, with the borrow
site and the invalidating site both named. That is the same treatment raw
pointers get, and it is interpreter quality rather than a language
guarantee.

The backstop reasons about *parts*, not whole values: two borrows collide
only when the storage they name overlaps. So `w.a.&mut` and `w.b.&mut` are
independent and may both be live, while a borrow of `w` and a borrow of
`w.a` are not — one contains the other. Reads and writes are judged the
same way, so neither `w.b = 5;` nor `let v = w.b;` disturbs a live borrow
of `w.a`.

Four limits of that backstop are worth stating plainly rather than leaving
to be discovered.

It is *dynamic*, so it reports a violation only on a path that actually
runs. A branch never taken is never checked, and a program that passes on
one input says nothing about another.

And its liveness notion is the FRAME, not the block. A borrow of a local
declared in an inner block keeps working after that block ends, because the
frame still owns the storage — the interpreter sees a live allocation and
has nothing to object to. Statically that case belongs to loan liveness,
which is the next stage. So a borrow of an inner-block local, read after its
block ends, is caught by NEITHER layer today: the outlives module does not
model it and the interpreter cannot see it. That shape stays uncaught until
the loan checker lands.

And the escape check's reach is the ENCLOSING ITEM's universals, not every
frame in the body. A nested function literal that returns a borrow of its
own local at `@_` escapes that literal's frame without ever reaching a
universal of the outer item, so the static checker sees it as clean —
the interpreter still catches it, because the local's storage really is
gone. Like the inner-block case above, this stays uncaught statically
until static loan liveness lands.

Finally, the reborrow is minted only where the position *wants* a borrow. A
borrow-typed place read into a position with no borrow-typed expectation — a
bare `let c = b;`, or a read of an affine field, `p.q` — copies the borrow
verbatim instead: no reborrow is minted, nothing is suspended, and the
interpreter sees one borrow where there are two, so both writes land.
Closing that means minting the reborrow (or a move) regardless of
expectation, which is a change to typing rather than to either checker.

Also not yet supported: regions on type declarations (`struct::<@a, T>`),
implied bounds, elision of any kind, and compiling a program that uses safe
borrows to wasm — the backend refuses those by name rather than dropping the
contract silently. See `examples/borrows.must`.

== Arrays

Fixed-size arrays `[T; N]` — the length is part of the type. Literals
(`[1, 2, 3]`), the repeat form (`[e; N]`), indexing (`a[i]`) and
index-assignment (`a[i] = v;`, whose `mut` requirement is transitive from
the root, like fields). Nested arrays are arrays of arrays; the element type
may be a record, a variant, anything.

```
static main = fn () -> usize {
    let mut a = [1, 2, 3];
    a[0] = 10;
    let m = [[1, 2], [3, 4]];
    a[0] + a[2] + m[1][0]         // 16
};
```

Length is typed exactly: `[usize; 2]` and `[usize; 3]` never unify — not
across a call, not against an annotation — so a length mismatch is a
check-time error naming both lengths. Because the length is known, an index
the checker can *evaluate* is bounds-checked right then: `let a = [1, 2];
a[2]` is a compile-time squiggle reading `index out of bounds: the length is
2 but the index is 2`. The identical text is what a genuinely runtime-only
index traps with — squiggle text and trap text are one render.

Arrays are const-legal, which is the flagship pattern: build a table
imperatively in a `const { ... }` block and *freeze* the finished value into
a static.

```
static table = const {
    let mut t = [0; 5];
    let mut i = 0;
    loop {
        if i == 5 { break t; };
        t[i] = i * i;
        i = i + 1;
    }
};                                 // table: [usize; 5] = [0, 1, 4, 9, 16]
```

Const-param lengths connect arrays to generics: a generic function's `const
N: usize` may be the length of a `[usize; N]` parameter, and a generic type
may carry a `[usize; N]` field (`Buf::<2>` above). Not yet: a length
accessor, and matching on arrays.

== Running compiled modules

A compiled module expects exactly one import, `must.print(ptr, len)`, and
exports one function, `main` (the entry expression compiled in, chosen
with `must-lsp compile -e <expression>`, default `main()`). Two tools in
`tools/` run one: `wasm-run.mjs` from the command line, `playground.html`
by opening it in a browser and dropping the file on it — `file://` works,
no server needed. Neither is a WASI runtime: a general-purpose engine such
as wasmtime or wasmer will not run these modules as-is.

On a clean return both print the raw ABI result slots — a `.wasm` file
carries no type information, so this is not the typed `Display`
`must-lsp run` gives. On a trap both decode which one fired and why, from
the module's `trap_code`/`panic_message_*` globals (also read by the
differential test harness) and its `must.traps` custom section, which
exists so a host with no access to the compiler can still name the trap.

This backend has no heap and no raw pointers yet, so `examples/heap.must`
and `examples/pointers.must` refuse to compile rather than miscompiling —
there is nothing for either tool to run for those two examples.

Monomorphization has refusals of its own. A program whose instantiations
never bottom out — polymorphic recursion, where every call needs an
instance the caller did not — is named after a fixed number of
re-entries on the instantiation path. Those re-entries are counted
across a whole mutual cycle rather than per item, so a cycle is caught
about as early as direct self-recursion is — unless the cycle is long
enough that the path reaches the depth cap before the re-entry count
does, in which case it is refused under that cap's message instead. That
cap is the next refusal: a call chain too deep to walk inside the stack
budget the backend documents, recursive or not, is refused by depth
alone. And a program that instantiates too many functions in total,
however shallowly, is refused by count.
