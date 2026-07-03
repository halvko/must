= Must lang

Consistent function/closure syntax:
```must
static main: fn() -> () = fn() -> () {
    let s: str = "hello";
    (fn (s: str) -> () { print(s) })(s);
};
```
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

Tagged types can be created:

```
trait Eq {
    type Lhs = Self;
    type Rhs = Self;
    eq: fn (lhs: Lhs, rhs: &Rhs) -> bool = {
        fn<S: Struct>(lhs: S, rhs: S) => {
            // ...
        }
        fn<E: Enum>(lhs: E, rhs: E) => {
            // ...
        }
    }
    /*
    typematch (Lhs, Rhs) {
        (s { ...lhsFields }, s { ...rhsFields }) => for field in lhsFields {
            rhsFields[field] == lhsFields[field]
        }
    }
    */
} derive for Struct<...T> where T: Eq {

} derive Enum {

}

type S {
    a: string,
} with self {
    new = fn (a) => S { a }
} with Eq {};

type Nested {
    a: { b: string }
};

type HasGeneric<T> { phantom: Phantom<T> } with Eq where T: Eq {};

type ExampleEnum | A | B;

static main = fn {
    let s1 = S::new("example");
    let s2 = S::new("example");
    assert(s1 == s2);
}
```

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
static x = const { let mut n = 1; n = n + 1; n };
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

```
type Point = struct { x: usize, y: usize };
```

`Point` unifies only with itself: same declaration or nothing. There is no
implicit coercion between a named type and its underlying record shape, in
either direction — a `Point` is not a `struct { x: usize, y: usize }` and
vice versa (the error explains the underlying shape and how to construct).

Construction is a plain function call: the type name applied to the
underlying record value. Constructors are functions — no special brace
syntax on the type name.

```
static origin = Point(struct { x: 0, y: 0 });

static translate = fn (p: Point, dx: usize) -> Point {
    // Field access projects through to the declared shape.
    Point(struct { x: p.x + dx, y: p.y })
};
```

A bare `Point` in expression position is an error (`Point` is a type, not a
value); the construction head is its one legal expression position.
Construction is pure, so it is legal in const contexts.

At runtime named types are fully erased: a `Point` value *is* its record
value — same representation, structural equality under the hood. The type
system alone keeps `Point` and bare records apart, so erased equality is
only ever asked between two values of the same nominal type.

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
