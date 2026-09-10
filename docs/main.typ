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

== Comments

`//` runs to the end of the line. `/* ... */` runs to its matching `*/` and
*nests* — a `/*` written while a block comment is already open starts another
level, so commenting out a chunk of code that itself contains a block comment
just works:

```must
/* static old = fn {
    /* print("debug"); */
    1
}; */
```

The inner `/* print("debug"); */` doesn't end the outer comment early; only
the final `*/` does. An unclosed `/*` eats the rest of the file as one
comment — there's no good place to resume after it — and the error(s) name
every opener still unclosed, not just one, so nesting two levels deep and
forgetting both `*/`s gets two diagnostics, not a single confusing one.
`/**` is not a doc-comment marker — Must has no doc-comment convention (yet),
so `/** like this */` is just an ordinary comment whose first character
happens to be `*`.

== Statements and semicolons

An expression statement ends with `;` — unless the expression already ends with
`}`. `if`, `match`, `loop`, `unsafe { ... }`, `const { ... }` and a plain block
all close themselves in statement position:

```must
static classify = fn (n: usize) -> str {
    let mut out = "many";
    if n == 0 {
        out = "none";
    } else if n == 1 {
        out = "one";
    }
    out
}
```

No `;` after that chain's last `}`. It is the same rule items already follow —
a value ending in `}` needs no separator — reaching one level in. Writing the
`;` anyway is still fine (`if c { };` parses, and means exactly the same
thing); it just isn't needed. Everything that does *not* end in `}` still needs
its `;`:

```must
static shout = fn (s: str) -> () {
    print(s);
    print("!")
}
```

The rule is about the last *token*, not about a list of special forms, so
anything ending in `}` self-terminates in statement position — the shapes
above and whatever is added later.

Two statements keep their `;` even though they end in `}`, and it's worth
knowing which before you meet them: `let` and assignment. Their `}` closes a
*value*, and the statement wrapped around that value still needs its terminator:

```must
static pick = fn (c: bool) -> usize {
    let mut out = 0;
    let first = if c { 1 } else { 2 };
    out = if c { 10 } else { 20 };
    first + out
}
```

Both of those `;`s are required.

One more place the `;` earns its keep: the *end* of a block. Written without it,
a `}`-ended expression in the last position is the block's tail — its value —
rather than a statement whose value is thrown away. The two spellings are
interchangeable everywhere else; there, drop the `;` only when the value is `()`
or when the tail is what you meant.

=== A `}` does not stop an expression

Newlines mean nothing to the parser, and an operator after a `}` continues the
expression that `}` closed. So this is one subtraction, not a branch followed
by a negation:

```must
static discounted = fn (bulk: bool) -> usize {
    if bulk { 100 } else { 120 } - 1
}
```

That is deliberate: a sequence of tokens has one reading, not one reading in
statement position and a different one in expression position. The cost lands
on a rare shape — a statement that genuinely *starts* with `-`, `(` or `[`,
directly after a statement that ended in `}`, needs a `;` above it to say so:

```must
static twice = fn (n: usize) -> usize {
    if n == 0 { print("zero") } else { print("more") };
    (n) + n
}
```

Drop that `;` and `(n)` stops being a new statement: it *calls* the `if` above
it.

Whenever a continuation like this sits on a different line from the `}` it
continues — the shape that reads as two statements and parses as one — the
compiler warns and names both ways out: insert the `;` to split them, or move
the operator up onto the `}`'s line, or wrap the whole expression in
parentheses, to say that one expression was meant. On the same line it stays
quiet: written next to the `}`, a continuation is obviously deliberate.

The `;` also comes as a quick fix — but only where splitting would leave a
program. `-`, `(` and `[` can begin a statement, so those get the button; a `*`
or a `.` cannot begin anything, so there the warning explains and leaves the
edit to you.

The same warning covers match arms, where `,` plays the `;`'s part — without it
`_ => { 1 }` on one line and `- 1,` on the next is a single arm whose body is
`{ 1 } - 1`, which type-checks and says nothing. No quick fix is offered there
yet: no *continuation* token can begin a pattern, so the `,` would only trade
the confusion for a parse error.

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
`print` or `read_line` in a const context is an error; calling `panic` is
allowed — failing loudly at compile time is the point of putting code there.

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
    if n > 10 { n = 10; }
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
reachable through the dot exactly when its LAST parameter is `Self`-typed —
or a safe borrow of `Self`, which is its own section below — and the
receiver becomes that last argument. `c.bump(2)` means `bump(2, c)`
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
member's `Self`, with one exception: a borrow receiver reborrows into a
member whose `Self` is itself a borrow, and nothing else is ever inserted.
A *raw* pointer to a type with members still never dot-calls them; write
the deref yourself.

The owner's generic binder is in scope in member signatures and bodies, and
a dot-call never spells the OWNER's arguments — the receiver's type supplies
them; a member's own type parameters are a separate list (see "Members with
their own type parameters"):

```must
type Box2 = struct::<T> { v: T } with {
    impl Self {
        get = fn (b: Self) -> T { let Box2(struct { v }) = b; v };
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
errors. On a non-enum scrutinee only `_` or a binding can match (for
now) — the scalars are the exception, since their literals are patterns
too (see "Scalar literals are patterns" below).

*Matching a BORROW projects through it.* A scrutinee of type
`Opt::<T>.&::<@a>` dispatches on the enum behind the borrow, and each payload
binding comes out as a *borrow of that payload's slot* rather than a copy of
it — `.&mut` in, `.&mut` out. Nothing in the pattern says so: the scrutinee's
flavor decides, there is no per-binder marker, and patterns stay exactly
construction-shaped.

```must
type Opt = enum::<T> { Some(T), None };

static bump = fn::<@a>(o: Opt::<usize>.&mut::<@a>) -> () {
    match o {
        ::Some(t) => { t.* = t.* + 1; },   // `t: usize.&mut` — writes the OWNER
        ::None => {},
    }
};
```

That is what makes an `as_ref`-shaped function writable — one that turns a
borrow of an option into an option of a borrow:

```must
static project_in = fn::<@a, T>(s: Opt::<T>.&::<@a>) -> Opt::<T.&::<@a>> {
    match s {
        ::Some(t) => Opt::<T.&::<@a>>::Some(t),
        ::None => Opt::<T.&::<@a>>::None,
    }
};
```

The payload binding's region is fresh, with one rule attached: the
scrutinee's region must outlive it. So it may be *shorter* than the
scrutinee's and can never be longer — and, as above, it may be exactly the
scrutinee's, which is why `project_in` can hand its result back at the
caller's own `@a`. Trying to return it for longer is a check-time error
naming the bound the signature would have to declare.

Because every binding is a borrow, you can never *move* a payload out of a
borrowed scrutinee. Reading one is the ordinary `t.*`, which copies, so the
payload has to be copyable; a payload that must move needs the owned place
matched instead. The mismatch you get for using a binding where the value
is wanted names both routes. Matching an *owned* scrutinee is unchanged in
every respect — it still copies, and still moves payloads that cannot be copied.

Since a binding is itself a borrow, matching *it* projects again, which is
how the rule reaches nested data today (nested patterns are not grammar
yet). And the tag test is a real read *through* the borrow, so a scrutinee
that has already been invalidated is caught at the `match` itself rather
than at whichever arm first touches a binding. A `match` no arm of which
dispatches — only `_` or a binding — tests no tag and so reads nothing.

*Matching a variant-typed scrutinee needs no dispatch at all.* A `fn (s:
State::Running)` knows statically which variant it holds, so a `match`
inside compiles to a direct payload destructure — no switch, no tag read,
the enum fully erased. That is the state-machine payoff: code that stays
on one variant pays nothing for the enum it belongs to. Arms naming the
*other* variants of the enum are legal but flagged unreachable.

`match` is pure control flow, so it is const-legal — fine inside `const
fn` bodies and `const { ... }` blocks.

*Scalar literals are patterns* — characters (`'(' => ...`, see "Characters"
below) and integers (`0 => ...`). They dispatch by equality rather than by
a tag, and a scalar scrutinee always needs a `_` arm. That is a rule, not
an arithmetic: a `u8` could be covered by listing 256 arms and Must still
asks for the `_`, because covering a scalar by enumeration is not a thing
a program does.

An integer literal pattern *takes its type from the scrutinee*, exactly as
an integer literal in expression position takes its type from context.
There are no suffixes and no default width: the same written `0` is a `u8`
in one match and an `i64` in the next, and a literal the scrutinee's type
cannot hold is the ordinary out-of-range error (`300` matched against a
`u8`). A match on a value whose own type nothing pins gets the ordinary
"no defining use" answer on the pattern — nothing defaults.

```must
static classify = fn (d: usize) -> str {
    match d {
        0 => "zero",
        1 => "one",
        _ => "many",
    }
};
```

Not in v1 (landing later as one coherent pattern-language feature): nested
patterns, or-patterns (`0 | 1`), guards, *negative* literals (`-1` is an
operator applied to a literal, so taking it here would be the first step
of a pattern *expression* grammar), *ranges* (`0..=9` — a separate
question, and a deeper one: it asks what order a scalar has, and whether
listing ranges could exhaust a type and so retire the `_` arm), literal
patterns of the *other* kinds (string, boolean — they parse, so the error
names the kind that was written rather than shrugging), `if match`,
`match ... else`, the statement form `match x => pat;`, and record
patterns — though `..` is already reserved in pattern position for them.
Nested patterns are the one whose absence is visible above: reaching into
nested data through a borrow is spelled as two matches until they land.

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
        if i == 10 { break acc; }
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
    if n == 0 { return "zero"; }
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

That slot settles more than a diverging body. A literal written where a
`fn` type is expected takes its unwritten parameter *and* return types
from the slot before its body is checked, so with
`apply = fn (g: fn (Counter) -> usize, c: Counter) -> usize` the call
`apply(fn (t) { t.n }, c)` needs no annotation on `t` — the slot's
parameter types have to be known already, which a generic `apply` waiting
on a later argument for them would not be. It also extends the sigil's
own list of positions: the body of a `fn` literal that itself sits at
one of those now has an expected type too, so a call argument typed
`fn () -> Shape` lets `::Point` type-check inside `fn () { ::Point }`
written right there, with nothing annotated.

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

A type's binder is not the only one a member sees: an inherent member may
bind type parameters of its own, which vary per *call* rather than per
value — see "Members with their own type parameters" below.

== Raw pointers and unsafe

Raw pointers reach places all the way down. The types are `T.&raw`
(shared) and `T.&raw mut` (mutable). An address is taken with `place.&raw` /
`place.&raw mut`, and a place is now the full grammar: a variable, a chain
of its fields and elements (`r.a`, `a[i]`, `a[i][j]`), a `static` or `const`
item (a `const` use's own copy), or a chain rooted in a deref (`p.*.x` — a
place reached *through* a pointer). An operand that is not a place is
materialized as a temporary first (see "Borrowing a temporary" below). A
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
    unsafe { p.* = 42; }
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

=== Unsafety is part of a function's type

`unsafe fn(usize) -> usize` is a type, distinct from `fn(usize) -> usize`.
Calling a *value* of it needs the marker, wherever that call happens:

```must
static apply = fn (g: unsafe fn(usize) -> usize, x: usize) -> usize {
    unsafe { g(x) }
};
```

Why the type and not the declaration: once a function is bound to a name,
passed as an argument or stored in a field, the call site says only that
*something* is being called. The declaration is nowhere in sight. The type
is the one thing still travelling with the value, so the type is what
carries the obligation — through `let`, through parameters, through returns,
through record fields, through a generic instantiated at it.

Taking such a value costs nothing. Binding it, passing it and returning it
run no code, so none of them needs a marker; only the call does.

Conversion goes ONE WAY. A safe `fn` may be used wherever an `unsafe fn`
is expected — a function that needs no vouching is welcome in a position
willing to vouch — and never the reverse:

```must
static twice = fn (x: usize) -> usize { x + x };
static ok = fn () -> usize { apply(twice, 21) };        // safe -> unsafe
static risky = fn (g: unsafe fn(usize) -> usize) -> usize {
    let safe_only: fn(usize) -> usize = g;              // type mismatch
    safe_only(1)
};
```

The refusal is an ordinary type mismatch, and it renders both types in
full — which means the two lines it shows differ by exactly the one token
that is the whole story.

Two kinds of function have an `unsafe fn` type without anyone writing one:
a host import (the "Host imports" chapter), and the builtins that both
require the marker and have a function type at all — `dealloc_array` and
`str_bytes`. (`copy`, `add`, `offset` and the two blesses are polymorphic
in a pointer's flavor, so they have no single `fn` type to carry anything;
they are not values, and never were.) That first pair closes a real hole:
`let d = dealloc_array::<usize>; d(p, 1);` used to run with no marker
anywhere, because the check looked at the *name* being called, and once the
builtin was a value there was no name left to look at.

To give a function of your own an `unsafe fn` type, annotate the value:

```must
static f: unsafe fn(usize) -> usize = fn (x: usize) -> usize { x };
static call_f = fn () -> usize { unsafe { f(1) } };
```

The marker on the literal itself — `unsafe fn (x: usize) -> usize { ... }`
— is still refused. It is the annotation that would be sugared away, and it
declares nothing the type does not already say.

None of this reaches the backend: `unsafe fn` is a check-time distinction,
and the same program with and without the marker compiles to the same bytes.

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
            }
            unsafe { dealloc_array(p, 2); }
        }
        AllocResult::Err => print("out of memory"),
    }
};
```

== Safe borrows and regions

A borrow is a checked pointer. `x.&` borrows a place for reading, `x.&mut`
borrows it exclusively, and `r.*` reads through either — the same postfix
deref a raw pointer uses. What makes them different is that a borrow needs
no `unsafe`: safety is decided by the pointer's *flavor*, and `.&raw` is the
unsafe flavor. There is no auto-ref and no auto-deref, ever. The old prefix
spelling (`&x` / `&mut x`, and the type forms `&T` / `&mut T`) is retired
the same way `&raw` was — a targeted migration diagnostic, never a silent
reinterpretation — and here the diagnostic also carries a fix that rewrites
it postfix, except where postfix would bind somewhere else (a borrow of a
`fn(..) -> T` has no postfix spelling at all).

Every borrow says how long it is good for. That is its *region*, spelled
`@a`, and it is written where types are written — on the borrow *type*'s
own turbofish:

```must
static get = fn::<@a>(r: usize.&::<@a>) -> usize {
    r.*
};
```

`@a` is a parameter of `get`, exactly as `T` would be, and it rides the same
binder list — `fn::<@a, T, const N: usize>` declares one of each, and the
regions come first in it. Regions are nonetheless a *distinguished* kind:
they are erased before anything is compiled. Two functions whose signatures
differ only in their regions lower to identical code, no region reaches a
monomorphization key, and no backend ever sees one. A region can therefore
reject a program and can never change what it does.

"Where types are written" is the whole rule: a region appears only in a
type position, and an operation never carries one. The borrow `x.&` is an
operation, so `x.&::<@a>` and `x.&mut::<@_>` are refused, with a fix that
drops the argument. A body that wants to pin a borrow's region says so in
an annotation, and the general form leaves the referent as a hole:

```must
let q: _.&::<@a> = p.*.&;    // the assertion, checked like any other
let inner = m.*.&mut;        // nothing to assert, nothing to write
```

The hole matters for an fn-typed value, where `fn() -> usize.&::<@a>` would
bind the region to the return type. The annotation is a checked assertion,
not a comment: over a local it reports that the borrow outlives its
storage, and against a shorter-lived parameter it reports the missing
outlives clause.

Nothing is elided in a *signature*. Every region a signature binds is
written by hand, on purpose, until enough real code exists to say which
elision rule would have earned its keep — so `usize.&` on its own is an
error naming the spelling rather than a guess. Inside a *body* the situation
is different: there the regions are inference variables, not parameters, so
`@_` says "there is a region here, work it out" in an annotation, and a
borrow with no annotation at all means the same. `@_` in a signature is
rejected: a signature's regions are parameters, and the binder is where a
parameter gets the name its outlives clauses and its other mentions refer to
— `@_` there leaves the contract unstated.

At a *call site* regions are elided entirely. A written turbofish spells the
callee's type and const arguments, in order, and nothing else:

```must
static get_first = fn::<@a, T>(r: T.&::<@a>) -> T.&::<@a> { r };

static main = fn::<@b>(p: usize.&::<@b>) -> usize {
    get_first::<usize>(p).*
};
```

Writing a region there — `@a` or `@_` — is refused, the borrow's rule
again, one argument at a time, so the rest of the list still counts. There
is nothing for it to pin: the callee's regions become fresh existentials of
*this* call, which the borrow checker solves from the arguments actually
passed, so `@_` would be a token meaning "as before" on every borrow-taking
generic call in the program. A type argument can be genuinely undetermined;
a region never is. This is also why regions come first in a binder: what a
turbofish spells is the binder minus its regions, and that subtraction only
reads correctly when the elided part is a contiguous prefix — `fn::<T, @a>`
is a syntax error naming the fix. The one list none of this reaches is a
*type's* own (`Pair::<usize>`, as a type or in a construction call): a type
declaration binds no region yet, so that list is positional over its whole
binder and a region written in it is simply a wrong argument.

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

`.&mut` additionally requires the root binding to be `mut` — the same
transitivity rule assignments use — and blames the root with the same "make
it `mut`" quick fix an assignment would. Nor may write permission be reached
through a shared step, which is what stops a shared borrow laundering into
one. That is judged over the whole place rather than its outermost step:
writing through a `.&` is rejected (`cannot assign through `T.&`: writing
needs a `.&mut` borrow`), and so is writing through a `.&mut` that is itself
held behind a `.&`, because reading a `.&mut` out of a place *reborrows* it,
and a shared place may grant no such reborrow. Minting `.&mut` or `.&raw mut`
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

Using a borrow where the value itself is wanted is a plain type mismatch,
and its message names both ways out: `.*` to copy through the borrow, or
the owned value in place of a borrow of it.

=== Members that borrow `Self`

A member is dot-callable when its *last* parameter is `Self`-typed — and a
safe borrow of `Self` counts, which is what lets a type expose accessors
rather than consuming its receiver. Such a member declares the region it
borrows for, on its own binder:

```must
type Counter = struct { n: usize } with {
    impl Self {
        get  = fn::<@b>(m: Self.&::<@b>) -> usize { m.*.n };
        bump = fn::<@b>(m: Self.&mut::<@b>) -> () { m.*.n = m.*.n + 1; };
    }
};

static twice = fn::<@a>(c: Counter.&mut::<@a>) -> usize {
    c.bump();
    c.bump();
    c.get()
};
```

`@b` has nowhere else to come from: the type's own binder carries its type
parameters, not regions, and a signature elides nothing. It is a fresh
region at every call, so the two `c.bump()` calls above borrow
independently — and nothing at those call sites names it, because a call
site never spells a region.

`c` there is already a borrow, and `c.bump()` does not dereference it —
there is no auto-deref. It *reborrows*: the receiver is the last argument
like any other, so it gets the same insertion every argument position gets,
which is the licensed exception above (`c` is already a borrow, so borrowing
`c.*` is allowed). An exclusive receiver reaches a shared member the same
way, by degradation.

Going the other direction is refused, and by the same rule read backwards.
On an *owned* receiver the compiler would have to insert a borrow of the
local itself, which the exception forbids — so you write it, and
`counter.&mut.bump()` is an ordinary postfix chain whose receiver is then a
borrow. A shared receiver never reaches a `Self.&mut` member at all, because
shared never becomes exclusive. And a borrow receiver never reaches a member
whose `Self` is a *value* — that would be auto-deref; write `c.*.take()`.

=== Borrowing a temporary

`.&` and `.&mut` borrow a *place*, and a freshly computed value is not one.
The compiler gives it storage: an anonymous local nobody can name, created
where the expression is written.

```must
static forty = fn () -> usize { 40 };
static get = fn::<@a>(r: usize.&::<@a>) -> usize { r.* };

static main = fn () -> usize {
    get(forty().&)    // a temporary, borrowed: 40
};
```

The *whole* value gets the storage, so a borrow of a field or element
(`mk().a.&mut`) points into the temporary, and a write through it lands
there. The expression is evaluated where it is written: operands keep their
order, and a temporary in a `match` arm or a loop body is created on the
path that runs, each time it runs. A `const` context works the same way.

A temporary lives to the end of the innermost enclosing block; a shorter
life is spelled with an explicit block. A borrow of a temporary that has to
outlive the body is refused as `borrowed value does not live long enough`,
naming the temporary.

`.&raw` and `.&raw mut` materialize the same storage. One thing is refused
under every flavor: a temporary of a type that *must be consumed*. It has no
name, so nothing could consume it; bind it with `let` first. A *name* is
never materialized, whatever it resolves to.

=== Members with their own type parameters

A member may also bind *type* parameters of its own, beside its regions.
The reason is not sugar: a member-own type parameter varies *per call*, and
no binder on the owning type can say that.

```must
type Option = enum::<T> { Some(T), None } with {
    impl Self {
        flat_map = fn::<U>(f: fn(T) -> Option::<U>, s: Self) -> Option::<U> {
            match s {
                ::Some(v) => { f(v) }
                ::None => { Option::None }
            }
        }
        map = fn::<U>(f: fn(T) -> U, s: Self) -> Option::<U> {
            s.flat_map(fn(t) { Option::Some(f(t)) })
        }
    }
};
```

`T` is the container's — one per `Option::<T>` — while `U` is the call's:
`map` over the same `Option::<usize>` may produce an `Option::<bool>` at
one site and an `Option::<str>` at the next. `examples/option.must` is this
type in full — `is_some`, `unwrap` and `as_ref` beside `flat_map` and `map`.

The member's binder is the owner's *followed by* its own, which is what
makes the two halves reach the call site from different places. The owner's
arguments come from the receiver's type; the member's own are inferred, or
written in a turbofish on the member's own name — in either spelling:

```
o.map::<bool>(width)                     // the dot spelling
Option::map::<bool>(width, o)            // the qualified spelling
Option::<usize>::map::<bool>(width, o)   // both binders, spelled out
```

That written list spells the member's *type* parameters, in order, and only
those. Its regions are not positions in it — the same rule a free
function's turbofish follows, for the same reason: a member's regions are
fresh at every call and are solved from the arguments passed, so there is
nothing at the call site for a written one to pin. A member with no binder
of its own takes no arguments at all — including an empty `::<>` — and says
so.

The literal in `map`'s body is not annotated, and does not need to be: a
`fn` literal takes its parameter and return types from the position it sits
in, so `fn(t) { Option::Some(f(t)) }` gets both from `flat_map`'s parameter.
That is a general rule, not a member one.

Everything else follows the generics chapter unchanged. The body is checked
*once*, with `U` rigid — so `U` supports nothing until bounds arrive, and
`flat_map`'s "hand it back or pass it on" shape is exactly what a rigid
parameter admits. `U` asks for nothing unless the body needs something: a
member that DISCARDS its `U` writes `U: forget`, and that promise is
checked where the call spends the binder. And a member's own
name may shadow one of the owner's, last declaration winning, the same rule
the binder's const parameters already follow. Where the two then meet, the
mismatch says which binder position each `T` is, because both render the
same way.

Const parameters on a member are *not* supported yet, and the reason is
narrower than "sugar": a const argument is part of an instance's identity,
and the one place a member's arguments are carried reads them off the
receiver's own type — which cannot supply something the receiver does not
have.

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

=== What is checked

The borrow checker has two halves, and both are static. The outlives
module collects each body's outlives obligations, solves for every region's
value, and rejects an undeclared relation between signature regions and a
borrow of a local that escapes. Loan liveness runs over the lowered
control-flow graph and rejects a borrow that is still needed after the
value it borrows was touched behind its back. Both are pure per-body
queries whose entire output is diagnostics; nothing they compute is
consumed by lowering, layout or selection.

The rule of the second half is one sentence: no borrow may still be live
where something touches an overlapping part of the same place behind its
back. *Live* means used again later, not still in scope — a view you have
finished with is dead where you finished with it, which is what makes a
loop that reads a line, uses it, and reads the next one perfectly ordinary
code.

Four things count as touching it behind its back, and they are the same
four the interpreter enforces. Three of them are *writes*, and a write
invalidates every borrow of an overlapping part, shared or exclusive:

Using the place *mutably* — writing `x.&mut`, or simply mentioning a `.&mut`
you already hold, which mints a fresh reborrow.

*Assigning* to it — `n = 99`, or `r.*.f = v`. The borrow would read storage
the assignment has already overwritten. Assigning to a borrow-typed local
is the one write that reaches nothing: `p = q` replaces the pointer, so
the borrows taken through the old one no longer stand in the way of the
new one — they stay usable, and keep protecting what they point at. It
also means you have finished with what `p` held, so the place that borrow
was of is free again — `r = b.&mut; a = 5;` after `r = a.&mut;` is
ordinary code, in a loop too.

*Moving* the value away. That takes the storage every borrow of it points
into. Disposing of a reader while one of its views is still needed is a
compile error, not a run-time trap.

The fourth is a *read* — taking a `.&` of the place, or just naming the
local — and it invalidates only an *exclusive* borrow, which is the point of
an exclusive borrow: while it lasts it is the only way to the value. Shared
borrows are untouched by a read; any number of readers coexist.

Parts, not whole values: two borrows collide only when the storage they
name overlaps. So `w.a.&mut` and `w.b.&mut` are independent and may both be
live, while a borrow of `w` and a borrow of `w.a` are not — one contains
the other. Reads and writes are judged the same way, so neither `w.b = 5;`
nor `let v = w.b;` disturbs a live borrow of `w.a`.

So the shape you were most likely to reach undefined behavior through — a
container with a borrow-returning member, the one this chapter just taught
you to write — is a compile error:

```must
type Cell = struct { n: usize } with {
    impl Self {
        slot = fn::<@b>(c: Self.&mut::<@b>) -> usize.&mut::<@b> { c.*.n.&mut };
    }
};

static two_writes = fn::<@a>(c: Cell.&mut::<@a>) -> () {
    let a = c.slot();
    let b = c.slot();   // refused: `a` is still live here
    a.* = 1;
    b.* = 2;
};
```

The refusal sits on the second use of `c` — the operation that cannot be
honored — and names the other two sites: where the first borrow was
created, and where it is still used. That is the same story the
interpreter tells when it catches one at run time, told earlier.

=== What the interpreter still adds

The interpreter's aliasing check is *depth*, not the fence. It runs the same
rule dynamically, and it reaches what the static rule cannot see: raw
pointers, freed allocations, and the places the checker approximates. A
refused program is not stopped from running — no trap is planted for a
refusal — and when it runs, this is what catches it. Its limits, and the
checker's, are worth stating plainly rather than leaving to be discovered.

It reports a violation only on a path that actually runs. A branch never
taken is never checked, and a program that passes on one input says
nothing about another. That is fine for what it is for.

Its liveness notion is the FRAME, not the block, and so is the checker's:
neither models the end of a block's storage, so a borrow of an inner-block
local, read after its block ends, is caught by NEITHER layer. It is the one
shape in this chapter that is neither rejected nor detected, and the end of
a temporary's block is the same shape.

In the other direction the static rule is deliberately stricter than the
interpreter on two points. Reading a place around a live exclusive borrow
only *suspends* that borrow at run time, so reading through it afterwards
still works; the checker refuses the read outright, because an exclusive
borrow you can read around is not exclusive. And every array index is
treated as possibly the same as every other, because `arr[i]` is not
something the checker can know — so a borrow of one element and a read of
another are refused together, though they would run fine. A rule can
always be relaxed later; it cannot be tightened without breaking programs
that already compiled, which is why both go the strict way.

One approximation is worth knowing about. A borrow stored into a local in
one arm of a `match` counts as live in the other arm too, whenever that
local is used after the match — even on the path where nothing was stored.
Handing it back to the caller instead, by returning it or by storing it
where the caller can see, is judged at the point it happens, so a lookup
that returns the borrow it found and inserts otherwise is ordinary code.

A nested function literal that returns a borrow of its own local at `@_` is
refused as well: the borrow is still live where the literal's body returns,
and the local is gone by then. That is the loan checker's finding rather
than the escape check's, which measures a borrow's reach against the
enclosing item's signature only.

Finally, the reborrow is minted only where the position *wants* a borrow. A
borrow-typed place read into a position with no borrow-typed expectation — a
bare `let c = b;`, or a read of an affine field, `p.q` — copies the borrow
verbatim instead: no reborrow is minted and nothing is suspended, so the
interpreter sees one borrow where there are two, and both writes land. The
checker treats that copy as a move of the holder, which refuses it while a
reborrow of the holder is still live, but two plain copies of one `.&mut`
are both usable; closing that means minting the reborrow (or a move)
regardless of expectation, which is a change to typing rather than to
either checker.

One more thing borrows do, covered where `match` is: matching a borrowed
scrutinee binds *borrows* of the payloads rather than copies of them, so
`Opt::<T>.&::<@a>` can be turned into `Opt::<T.&::<@a>>` and a `.&mut`
payload binding writes the value it was matched from. See
`examples/match_projection.must`, which runs.

Also not yet supported: regions on type declarations (`struct::<@a, T>`),
implied bounds, elision in a *signature* (call sites already elide every
region), and compiling a program that uses safe borrows to wasm — the
backend refuses those by name rather than dropping the contract silently.
See `examples/borrows.must`.

== Values that must be consumed

Some values stand for a resource — memory you asked the allocator for, a
handle the host gave you — and letting one go out of scope quietly is a bug,
not a shrug. Must does not solve that with destructors. It solves it by
refusing to compile the program.

A type declaration can say how far its values may go — its *ceiling*, the
most that can be done with one:

```must
type Res = struct {
    id: usize,
    size: usize,
} only move with {
    impl Self {
        drop = fn(r: Self) -> () {
            let Res(struct { id, size }) = r;
            release(id, size);
        };
    }
};
```

`only move` is the whole feature. `with` attaches things to a declaration
and `only` caps it, and they ride the same trailing slot in either order.
What it caps here is the *disposal* ladder, whose rungs are `access` (may
not even be moved), `move` (may be passed around, and that is all) and
`forget` (may be let go with nothing done about it). `access` is a rung the
ladder holds room for rather than a spelling that works yet — writing it
answers "the `access` capability does not exist yet". `forget` is the top and
the default, so a type that writes nothing keeps it; a type capped at `move`
must be *consumed*, exactly once, on every path out of every scope it lives
in.

`only` names a position on the ladder rather than an absence, and that is
what makes it survive company: a clause caps only the ladders it names, so
the day a `send` capability arrives, every `only move` type already written
keeps whatever the send axis defaults to. What it refuses is every clause
that would say one thing twice or say nothing at all: two rungs of one
ladder, because a declaration caps a ladder once; the same rung named again;
and `only forget`, which is the default, so it declares nothing.

Consuming is not a special operation. Passing the value to a function
consumes it, returning it consumes it, binding it to another name consumes
it, matching an owned value consumes it (and hands the obligation to
whatever the pattern bound). So does `r.drop()` above, which is an ordinary
method that takes `Self` by value — there is no `Drop` trait, no drop glue,
and nothing the compiler emits. `drop` is just the one everybody writes.

What that method does inside is the interesting part, because it is where
the obligation *ends*:

```must
let Res(struct { id, size }) = r;
```

Taking the value apart hands its obligation to its parts, and the parts are
two integers, which anyone may forget. That is the rule read backwards: a
record whose field must be consumed must itself be consumed, so a record
with nothing left in it that must be consumed is free. Containment is why
the ceiling spreads without being written twice —

```must
type Holder = struct { res: Res, tag: usize };
```

— `Holder` must be consumed too, and so must an `enum` with a `Res` in any
payload, and an array of them. A borrow of one, though, is an ordinary
value: `Res.&::<@a>` may be forgotten freely, because the thing that has to
be consumed is still where it was.

=== What the compiler refuses

Every refusal below is the same sentence at a different moment.

```must
static leaked = fn () -> () {
    let r = make(1);
};
```

answers "`r` is not consumed on this path; its type has no `forget`
capability, so every path must consume it". The squiggle is on the block —
the path that failed — with a note on `r`'s declaration, where the
obligation started.

```must
static twice = fn () -> () {
    let r = make(1);
    r.drop();
    r.drop();
};
```

answers "`r` was already consumed", with a note pointing at the first
`drop()`. That is the ordinary use-after-move error; for a value that must
be consumed it is also the double-disposal error, and it is the same check
either way.

```must
static one_arm = fn (c: bool) -> () {
    let r = make(1);
    if c { r.drop(); } else { }
};
```

answers "`r` is consumed on some paths through this expression and not on
others", reported on the whole `if`, because neither branch is wrong on its
own. Divergence is not a path: `if c { r.drop(); } else { panic("no"); }` is
fine, and so is a `return` that consumes on the way out.

```must
static every_time = fn () -> () {
    let r = make(1);
    loop { r.drop(); }
};
```

answers "`r` is left in a different state than the loop found it in": the
second iteration would consume it again. Putting a fresh value back
(`r.drop(); r = make(2);`) satisfies the loop, and so does moving the
declaration inside it.

The rule is about the *back edge* — the path that runs the body again — and
a `break` is not one. Taking ownership of something and stopping is
therefore the ordinary shape it looks like:

```must
static until = fn (n: usize) -> () {
    let mut i: usize = 0;
    let r = make(1);
    loop {
        i = i + 1;
        if i > n {
            r.drop();
            break;
        } else { }
    }
};
```

which is clean: the `break` leaves, carrying what it did with it, and there
is no next iteration to answer to.

A few smaller ones, each closing a way to make two obligations out of one
value — or to lose one: you cannot copy such a value out of a place (`h.res`
— take the whole thing apart instead), a `_` arm may not swallow one (give it
a name so it has somewhere to go), a `..` in a pattern may not skip a field
that must be consumed, `[r; 3]` is refused, and writing over a place that
holds one (`h.res = make(2);`, `m.* = make(2);`) is refused for the same
reason writing over a live binding is. Two places may not hold one at all:
a `static`, which is never destroyed, and a `const { ... }`, whose value is
computed once and copied into every evaluation.

One thing is *not* closed, and it is closed by `unsafe` instead. Raw storage
holds whatever you put in it: `alloc_array::<Res>(4)` is allowed — it hands
back a pointer and never holds a `Res` — so writing one through that pointer
and then freeing the buffer leaks it, and nothing says so. That is the same
hatch every raw pointer already is, and it is where it has always been: on
the other side of the marker.

=== In generic code

A type parameter says nothing about capabilities. `Box` below carries no
clause and no bound, and it holds a value that must be consumed anyway:

```must
type Box = enum::<T> { Full(T), Empty } with {
    impl Self {
        unwrap = fn(b: Self) -> T {
            match b {
                ::Full(t) => t,
                ::Empty => panic("empty box"),
            }
        };
    }
};
```

`Box::<Res>` must be consumed and `Box::<usize>` need not, and containment
is the whole of why: a `Box::<Res>` contains a `Res`, exactly as a struct
field does. There is nothing here for a declaration to opt into and nothing
for it to opt out of — one `Box`, both payload kinds.

The answer is read off the *declaration*, not off the instantiated type.
`Box` is summarized once — nothing in its own payloads has to be consumed,
and its `T` reaches a payload — so a `Box::<X>` has to be consumed exactly
when an `X` does. That is what makes a declaration which mentions itself
answerable: in

```must
type Pair = struct::<T> { l: T, r: T };
type Nest = enum::<T> { Cons(T, Nest::<Pair::<T>>), Nil };
```

`Nest::<usize>` may be dropped and `Nest::<Res>` may not, and nothing has to
expand the `Nest::<Pair::<T>>` beside it to say either. A parameter that
reaches no value position at all constrains nothing: every value position of
a `type P = struct::<T> { v: P::<W::<T>> }` is another `P`, whatever `W` is,
so a `P` never holds a `T` and no argument can make one linear.

Inside a generic *body* the parameter is checked as if it always had to be
consumed — the caller may hand it one, so the body may not assume otherwise.
That makes `unwrap`'s shape the shape that works: hand the value back, or
pass it on. A body that quietly drops a `T` on the floor is refused:

```must
static ignore = fn::<T>(t: T) -> () { };
```

answers "`t` is not consumed on this path, and `T` may be a type that must
be consumed; consume it, or write `T: forget` to require one that can be
discarded". The message names the *binder*, because inside a generic body
there is no declaration to send the reader to.

`T: forget` is that fix, and it is the one capability bound there is. It
rides the ordinary bounds slot, because what a body needs of its parameter
is a requirement like any other one it writes there. Writing it makes every
caller supply a type whose values may be dropped, so a body that discards a
`T` is fine and `ignore::<Res>(make(1))` is refused at the call instead.

What the bound does not grant is *duplication*. `T: forget` says a value may
be lost; whether one may be copied is a different question, and the answer
is the same for every rigid parameter, bounded or not: no. The language
supplies its own witness — a `T.&mut` may be forgotten freely and still may
not be copied, so a bounded `T` instantiated with one would hand out two
exclusive borrows of one place. A generic body therefore reads each of its
parameters once, and a second read answers "`x` was already consumed: a
value of `T` may not be duplicated, and no bound grants copying — borrow it
for the second use".

The rule belongs to the instantiation rather than to the binder, so it holds
without one: a concrete value that _holds_ an exclusive borrow — a
`struct::<T> { m: T }` at `usize.&mut` — is read once for the same reason,
and its refusal names the borrow instead of a parameter.

=== Borrows of a value that must be consumed

Borrowing one consumes nothing, so this is fine:

```must
static held = fn () -> usize {
    let r = make(1);
    let n = peek(r.&);
    r.drop();
    n
};
```

Holding the borrow *across* the consumption is not, and it is caught the
same way holding one across a write is:

```must
static stale = fn () -> usize {
    let r = make(1);
    let b = r.&;
    r.drop();
    b.*.id        // undefined behavior, detected
};
```

The aliasing model's event is "the storage no longer holds what the borrow
was taken of", and *moving* a value is that event as surely as writing over
it. The trap names both sites — where the borrow was created, and where it
stopped being good.

That check is dynamic, like every aliasing check today: the signature
discipline is what makes borrows *shaped* correctly, and the interpreter is
what catches a program that got the shape right and the order wrong.

None of this reaches the generated code. A program using values that must be
consumed compiles to exactly the same bytes as the same program without
`only move` on its declaration: the clause decides which programs are
accepted, and nothing else.

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
        if i == 5 { break t; }
        t[i] = i * i;
        i = i + 1;
    }
};                                 // table: [usize; 5] = [0, 1, 4, 9, 16]
```

Const-param lengths connect arrays to generics: a generic function's `const
N: usize` may be the length of a `[usize; N]` parameter, and a generic type
may carry a `[usize; N]` field (`Buf::<2>` above). Not yet: a length
accessor, and matching on arrays.

== Characters

`char` is a primitive holding one *Unicode scalar value* — a codepoint that
is not a surrogate. It is not an integer with a nicer name: a `char` has
equality and nothing else. No ordering, no arithmetic, no integer
conversions, because those are exactly the operations that would let you
build a value that is not a character. Equality is what reading text
actually needs.

Literals are written `'x'`, with the same escapes strings have and the
quote swapped: `\n`, `\t`, `\r`, `\0`, `\\`, `\'`. Only the delimiter that
would end the literal needs escaping, so `'"'` and `"it's"` are both
written plainly. Any single scalar value is legal, multi-byte included.

```must
static open: char = '(';
static newline = '\n';
static bullet = '•';
static same = 'x' == 'x';        // true
```

A character literal's type is *definite*, unlike an integer literal's:
there is one character type, so `'x'` is a `char` the moment you write it,
with no annotation and no defining use to wait for. There is no `{number}`
equivalent to leave unresolved.

Character literals are also *patterns*, which is the point of the feature —
scanning text is a `match` over characters:

```must
static classify = fn (c: char) -> usize {
    match c {
        '(' => 1,
        ')' => 2,
        _ => 0,
    }
};
```

A `char` match *always* needs a `_` arm — as policy, not arithmetic: a
`char` match is never exhaustive by enumeration, whatever the arms list.
The arms are tried in source order as equality tests — first match wins, a
repeated literal is an unreachable-arm warning.

Like every other pattern, a character pattern *projects through a borrow*
(see "Match" above): `match c { 'a' => ... }` means the same thing whether
`c` is a `char` or a `char.&`, and the comparison is a real read through
the borrow — so a scrutinee that has already been invalidated is caught at
the `match` itself, not at whichever arm first looked at it.

=== Walking a string

`str.next_char(i)` is the only way to look inside a string. It and
`s.len()` are the two builtins reached through a dot rather than by name.
It takes a *byte* index
and answers the compiler-provided `NextChar` enum, `Char(char, usize) |
End`: the scalar value starting at `i`, plus the byte index of the *next*
boundary — the value you thread into the following call. `End` means `i` is
at or past the end of the string. `NextChar` is minted per file exactly as
`ReadLineResult` and `AllocResult` are: an ordinary nominal enum, nameable,
matchable and shadowable.

There is no Iterator yet, so the walk is a `loop`/`match` idiom, the same
shape draining stdin has:

```must
static char_count = fn (s: str) -> usize {
    let mut n = 0;
    let mut i = 0;
    loop {
        match s.next_char(i) {
            ::Char(c, next) => { n = n + 1; i = next; },
            ::End => break n,
        }
    }
};
```

Threading the index is what makes the loop correct rather than merely
convenient: characters are not bytes. `"smørre"` is six characters and
seven bytes, and `next` is how the walk knows the difference. An index that
lands in the *middle* of a character is a program that lost track of where
it was, so it *panics* — it is neither `End` nor a silent slide to the next
boundary, because rounding it would turn a bug into wrong output.

`s.len()` answers the same currency the walk indexes in: the number of
*bytes*, so `"smørre".len()` is seven, not six. That is deliberate — every
other `str` operation counts bytes (`next_char` threads a byte index, both
blesses take a byte length), and a character count wearing the shorter name
would be a trap. Counting characters is the loop above.

Both are pure, so a `const` context accepts them — like the pointer
builtins, and unlike `print` and `read_line`, which have effects: you can
walk a literal at compile time and freeze the answer into a static. See
`examples/chars.must` for the whole picture — reading lines, counting
parentheses, and the byte-versus-character distinction in one program.

== Making a string out of bytes

Strings are not only literals. Given a pointer and a length you can claim
that the bytes there are text, and there are two ways to say it — one that
checks and one that does not:

```must
static text = fn (p: u8.&raw mut, len: usize) -> str {
    match unsafe { str_from_utf8(p, len) } {
        ::Ok(s) => s,
        ::Err => panic("not text"),
    }
};

static claimed = fn (p: u8.&raw mut, len: usize) -> str {
    unsafe { str_from_utf8_unchecked(p, len) }
};
```

Both are `unsafe`, and the reason is worth being exact about, because
"checked" names only half of it. That `p` addresses `len` readable bytes is
*your* claim in both spellings and nothing verifies it — that is the half
the marker is for. Whether those bytes spell a string is the other half:
`str_from_utf8` answers it and hands back the compiler-provided
`Utf8Result` enum, `Ok(str) | Err`, minted per file and shadowable exactly
as `NextChar`, `ReadLineResult` and `AllocResult` are;
`str_from_utf8_unchecked` assumes it.

Assuming wrongly is undefined behavior, and the interpreter catches it,
naming the offset of the first bad byte. That is not politeness — a `str`
whose contents are not a string is a value the language says cannot exist,
so a program that mints one has broken an invariant everything else relies
on, and finding out immediately is the only useful outcome.

The other direction is `str_bytes(s, dst)`: it writes `s.len()` bytes of an
existing string into storage you own. It is `unsafe` for the mirror-image
reason — that `dst` addresses that many *writable* bytes is your claim, and
nothing checks it — and between the two you can take a string apart and put
it back together without the compiler knowing anything about where the
storage came from.

The pointer is to `u8`, always: this is a bytes-first boundary, and a
claim about some other element type would be about layout, not text. Both
spellings take either pointer flavor, like `copy`'s source does, and both
are pure — so a `const` context accepts them too. A zero length is the
empty string and looks at no pointer at all, which is what lets a line
scanner bless a blank line with no special case.

== Reading standard input

`read_line()` is `print`'s twin — the stdin hook, a platform effect exactly
like `print` is (see "Running compiled modules" below for what that means
for a compiled module). It takes no arguments and returns the
compiler-provided `ReadLineResult` enum, `Line(str) | End`, minted per file
the same way `alloc_array`'s `AllocResult` is: an ordinary nominal enum,
nameable, matchable, and SHADOWABLE — in a file that declares its own
`ReadLineResult` the name resolves to yours, the same rule `print`'s name
follows (`read_line()` still returns the compiler's enum, matched with the
`::Line` / `::End` variant shorthands).

One call reads one line. There is no Iterator yet, so draining stdin is a
`loop`/`match` idiom — this one echoes its input back, a line at a time:

```must
static main = fn {
    loop {
        match read_line() {
            ::Line(s) => {
                print(s);
                print("\n");
            },
            ::End => break,
        }
    }
};
```

The trailing newline is STRIPPED — a CRLF terminator drops both bytes, so
input piped from either line-ending convention reads identically. A blank
line is real input, `Line("")`, never confused with `End`; `End` means
genuine end-of-input (the stream closed), not merely nothing available this
instant. A read that fails — on input that is not valid UTF-8, say — crashes
the program rather than returning: `ReadLineResult` has no error arm.
`read_line` is refused in a const context with `print`'s exact
message shape (const evaluation cannot have side effects) — see
`examples/errors.must`.

`must-lsp run` wires real, locked, line-buffered stdin through to a running
program, so the ordinary pipe invocation works:

```
printf 'a\nb\nc\n' | must-lsp run examples/stdin.must
```

For a file, redirection is the way to feed it — same wiring, no producer
process needed:

```
must-lsp run program.must < input.txt
```

A prompt written with no trailing newline still reaches the terminal before
the program waits: `read_line` flushes the output buffer before it reads.

Typing input by hand at an interactive terminal works too, with one caveat
that is the terminal's own, not Must's: a terminal holds one line at a
time, and a single line longer than it will hold is silently cut short —
many short lines paste fine. On an interactive terminal, `run` writes a
one-time note to stderr before the first read blocks:
`reading from stdin — end input with Ctrl-D`. Piped or redirected input
never sees it.

Two contexts have no real stdin to offer and say so honestly rather than
inventing one: the debug adapter (a debugged program's `read_line` always
reads `End` — there is no DAP console-input round trip to source a line
from) and the editor's ▶ run lens (its result is a toast message, not a
terminal). Neither hangs waiting for input that can never arrive.

`read_line` is the convenience, not the mechanism: "Standard input, as a
library" below builds the same line-reading behavior out of ordinary Must
code over one byte-moving host import.

== Host imports

`print` and `read_line` are builtins: the compiler knows their names. A
program can also declare a host import of its own, and the compiler learns
nothing at all about what it does:

```must
extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> isize;
```

Read that as what it is: a DECLARATION. There is no `=` and no value,
because an import does not set anything to anything — it promises that
something with this name and this type exists, and whatever provides it is
on the other side of the boundary.

THE DECLARATION IS THE WHOLE CONTRACT: the item's own name is the name the
host is asked for (a compiled module imports it as `must.read`,
`must.print`'s sibling), and the type written here is the one machine
signature the host must provide. An unwritten return type means `()`, since
there is no body to infer one from, and nothing in the type may be left to
inference either: a `_` in it would leave part of the contract unwritten,
with no body to write it. Everything refused is that same sentence from
another side — an `extern static` may not have an initializer, may not be a
`const`/`type`/`trait`, and must declare a function type, written in full.

The `unsafe` is written, not implied — `extern static read: fn(...)` is
refused. Two different things are unsafe about a boundary, and only one of
them has a spelling today. DECLARING a signature is already a vouch: if the
symbol out there is not shaped the way you said, the program is wrong before
anything calls it, and nothing on this side can check that for you. CALLING
is priced per function: `read` writes through your raw pointer, so it really
is `unsafe fn`, while a correctly declared `now: fn() -> i64` would be
perfectly safe to call. Must has no way to write the first vouch down yet, so
for now every import is declared `unsafe fn` — that way the reader sees at
least one sign that a boundary is being crossed. The safe-to-call import is a
real shape waiting on that marker, not a rejected one.

A DATA import (`extern static x: usize`) is a shape this spelling admits and
the language does not support yet — it is refused rather than guessed at.

The old spelling, `static read = extern fn(...) -> isize;`, is retired: it
put an `=` in front of something that is not a value. It still parses and
still means the same import, with a fix that rewrites it.

An import's TYPE is `unsafe fn(...)`, so calling one requires `unsafe` —
wherever the call happens:

```must
static f = fn (p: u8.&raw mut) -> isize { unsafe { read(p, 8) } };
static g = fn (p: u8.&raw mut) -> isize { let h = read; unsafe { h(p, 8) } };
```

The reason is the boundary itself. A raw-pointer deref needs the marker
because misusing it is undefined behavior; an import needs it because what
it does is written in a language this compiler never sees, so nothing on
this side can establish that calling it is sound. You vouch, which is what
the marker has always meant.

The type is what carries that from the declaration to the call. An import is
an ordinary function value — bind it, pass it, return it, put it in a record
— and every one of those is free, because none of them runs anything. The
call is where you say so, and the call always knows, because the obligation
came along in the value's type — see "Unsafety is part of a function's
type" under "Raw pointers and unsafe".

Calling one in a const context is an error for the same reason `print` is:
there is no host at compile time.

The *compiler* checks no import's signature against any host — one that did
would have to know every host, which is the coupling `extern` exists to
avoid. What happens instead is that each host answers for itself, and does it
thoroughly. The interpreter provides exactly one primitive, `read`, the
POSIX-shaped byte read: it fills the caller's buffer with at most `len` bytes
and answers a count, `0` at end of input, or a negative `-errno`. A short
read is real and is not end of input. The count comes back as a signed
machine word, so both `isize` (which is what POSIX calls it) and `i64` are
accepted and the answer arrives in whichever you asked for.

Declare it any other way — a buffer that is not a mutable pointer to bytes, a
length that is not a `usize` — and the call refuses, naming the signature this
host does have. That check is the declaration's, not the arguments': a buffer
of eight fresh `bool`s and a buffer of eight fresh bytes are indistinguishable
at run time, so only the type you wrote can say which one you meant.

An import the interpreter does not provide is refused by name when it is
called, rather than silently doing nothing:

```
runtime error: no host implementation for the import `launch_missiles`
  — the interpreter provides `read` and nothing else
```

A compiled module has its own reservation: `must.print` is already imported
for the builtin `print`, so an import may not claim that name — a module
cannot import one name twice.

Turning `read`'s bare count into something a Must program can match on is
library code, and `examples/stdin_lib.must` — the next section — is that
library.

== Standard input, as a library

`read_line()` is a builtin. It does not have to be — and
`examples/stdin_lib.must` is the proof: it reads lines from standard input
using nothing the compiler knows about except the ability to move bytes.

The host provides one primitive, POSIX's `read`, declared by the program
itself:

```must
extern static read: unsafe fn(buf: u8.&raw mut, len: usize) -> isize;
```

Fill a caller-owned buffer with at most `len` bytes; answer how many, `0` at
end of input, negative `-errno` on failure. No lines, no text, no policy.
Everything a reader actually does — buffering, finding a line boundary,
stripping a CRLF, deciding that a blank line is a line and that end-of-input
is not, moving a partial line to the front of the buffer to make room — is
written in Must, in that file, and you can change any of it.

Two details of the boundary are worth knowing because they are the ones
that make library code possible at all. The count comes back `isize` —
POSIX calls it `ssize_t`, and it is the type `offset` takes, so the end of
the filled region is `offset(p, n)` with nothing converted. And the line's
length comes out of the scan as an ordinary `usize` counter, because
finding the newline means looking at every byte anyway. Must has no integer
conversions, and this library needs none.

Turning the bytes into text is a separate, explicit step:

```
match unsafe { str_from_utf8(at, len) } {
    ::Ok(s) => s,
    ::Err => panic("stdin_lib: standard input is not valid UTF-8"),
}
```

And handing the line out is where the borrow rules do the work:

```
next_line = fn::<@b>(r: Self.&mut::<@b>) -> Option::<str.&::<@b>> { ... }
```

The line you get back is a BORROW OF THE READER. That is not documentation —
it is the type, and it means the reader can reuse its buffer freely, because
holding a line across the next `next_line` is not stale text but detected
undefined behavior, reported at the stale read with both sites named. Copy
what you need out (`line.*`) before asking for the next one, which is what
the demo does.

The limits are in the file, on purpose. The buffer is fixed, so a line
longer than it panics by name rather than truncating or hanging. Invalid
UTF-8 panics by name, because the checked bless made the library answer
rather than assume. And a line copied out of its borrowed view survives the
next refill only because `str` is an owned value today — "Owned strings"
below builds `String`, the shape that keeps working once that changes.

The reader itself cannot be dropped on the floor, either: `Reader` is
declared `only move`, so a path that never calls `drop` — leaking the
buffer it owns — is refused at check time, not merely bad style. A `panic`
never falls off the end, so a path that ends in one owes nothing. See
"Values that must be consumed" for what that check does and what it
refuses.

== Owned strings

`next_line`, above, hands back a *borrow* of the reader's own buffer, and
that borrow dies the moment the reader refills — which is correct, and
which is why the reader alone cannot answer "which line was longest?":
whichever line wins has to survive every refill after it, not just the one
it came from.

Copying the borrowed view out (`line.*`) works today only because `str` is
itself an owned primitive; the day a `str` becomes a real view onto a
buffer, that copy needs a real allocation behind it. `String` is that
shape, built now to show the capability doing the work: it is a *library*
type, not a compiler feature. Nothing below is built into the language:

```must
type String = struct {
    ptr: u8.&raw mut,
    len: usize,
    cap: usize,
} only move with {
    impl Self {
        as_str = fn::<@a>(s: Self.&::<@a>) -> str {
            unsafe { str_from_utf8_unchecked(s.*.ptr, s.*.len) }
        };
        len = fn::<@a>(s: Self.&::<@a>) -> usize { s.*.len };
        drop = fn(s: Self) -> () {
            let String(struct { ptr, cap, .. }) = s;
            if cap > 0 {
                unsafe { dealloc_array(ptr, cap); }
            }
        };
    }
};

static empty_string = fn() -> String {
    String(struct { ptr = unsafe { dangling::<u8>() }, len = 0, cap = 0 })
};

static to_owned = fn::<@a>(s: str.&::<@a>) -> String {
    let text = s.*;
    let n = text.len();
    if n == 0 { return empty_string(); }
    let p = match alloc_array::<u8>(n) {
        ::Ok(p) => p,
        ::Err => panic("out of memory"),
    };
    unsafe { str_bytes(text, p); }
    String(struct { ptr = p, len = n, cap = n })
};
```

`only move` is what makes it safe to write: a `String` must be consumed
on every path, so the compiler will not let you forget the `drop()`. See
"Values that must be consumed" for what that check does and what it refuses.

Two `str` operations do the copying: `s.len()` sizes the allocation (see
"Walking a string" above) and `str_bytes` fills it (see "Making a string
out of bytes" above).

Reading back uses the *claimed* bless, `str_from_utf8_unchecked`, and that
is not a shortcut: the bytes came from a `str`, so they are UTF-8 by
provenance, and `str_from_utf8` would be re-answering a question nothing
could have changed the answer to.

The empty string owns nothing at all — `alloc_array(0)` traps, so `cap == 0`
with a dangling pointer is how "there is no allocation" is spelled, and
`drop` checks for it before freeing.

=== Why it is worth the trouble

`m` below is a `.&mut` borrow of a `Reader` built with `reader_new`, taken
once outside the loop (`let m = r.&mut;`) so the same borrow reborrows on
every `next_line` call rather than being retaken each time:

```must
let mut longest = empty_string();
loop {
    match m.next_line() {
        ::Some(line) => {
            if line.*.len() > longest.&.len() {
                longest.drop();
                longest = to_owned(line);
            }
        },
        ::None => break,
    }
}
print(longest.&.as_str());
longest.drop();
```

`to_owned` is the copy that answers what the reader's own borrow cannot: it
survives every refill after the line it came from.

Notice the two lines in the middle. `longest.drop()` comes *before* the
assignment because writing over a live `String` would lose it, and the
compiler says so. And the loop is accepted only because it leaves `longest`
in the same state it found it in — dropping without replacing would mean the
second iteration disposed of something already gone.

`examples/string_lib.must` is this program in full, reader included.

== Running compiled modules

A compiled module's imports are `must.print(ptr, len)` plus whatever
`extern static` declarations the program itself made, and it exports one
function, `main` (the entry expression compiled in, chosen with `must-lsp
compile -e <expression>`, default `main()`). Two tools in `tools/` run one:
`wasm-run.mjs` from the command line, `playground.html` by opening it in a
browser and dropping the file on it — `file://` works, no server needed.
Both wire the builtin import and nothing else, so a program with imports of
its own needs a host that knows them. Neither is a WASI runtime: a
general-purpose engine such as wasmtime or wasmer will not run these
modules as-is.

On a clean return both print the raw ABI result slots — a `.wasm` file
carries no type information, so this is not the typed `Display`
`must-lsp run` gives. On a trap both decode which one fired and why, from
the module's `trap_code`/`panic_message_*` globals (also read by the
differential test harness) and its `must.traps` custom section, which
exists so a host with no access to the compiler can still name the trap.

This backend has no heap and no raw pointers yet, so `examples/heap.must`
and `examples/pointers.must` refuse to compile rather than miscompiling.
Every example whose `main` needs a heap-allocated buffer refuses for the
same reason, though not always at the same call: `examples/stdin_lib.must`'s
reader stops at `alloc_array` itself, while `examples/string_lib.must`'s
`main` calls `s.len()` directly and stops there first, before ever reaching
its own `alloc_array` call. A safe borrow is refused by name too, and
deliberately not folded into the raw-pointer refusal: a borrow lowers to the
same machine word, so this backend could emit something that runs while
silently dropping the exclusivity contract. `read_line` has no wasm import
yet either, so `examples/stdin.must` refuses by name — and so does
`examples/chars.must`, which reads a line before it walks it. `next_char`
has no wasm story either and refuses by name in its turn. Characters
themselves are no trouble: a `char` is one scalar slot here, so literals,
`==` and character-pattern dispatch all compile. The two `str` primitives,
`len` and `str_bytes`, have no wasm story either and refuse by name in
their turn. The two blesses refuse by name as well; neither
`stdin_lib.must` (stopped at `alloc_array`) nor `string_lib.must` (stopped
at `len`) gets far enough to exercise them.

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
