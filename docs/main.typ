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

Plain enums: e.g. the type `"hello"` is a valid type.

Plain structs: e.g. the type `{ a: string }` is a valid type (without a tag)

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

```
static normal_let = fn {
    let s2 = s;
}

static match_irrefutable = fn {
    match "hello" => s;
    print(s);
}

static if_match_some = fn => {
    enum Option<T> {
        Some(T),
        None,
    }

    let s = Some("hello");

    if match s => Some(s) {
        print(s);
    }
}

static match_else_never = fn => {
    enum Option<T> {
        Some(T),
        None,
    }

    let s = Some("hello");

    // btw, don't make me do trailing `;` after `match ... else { ... }`
    match s => Some(s) else {
        return;
    }

    print(s);
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

```
static example = fn ({n}: {n: usize}) => {
    print(n);
}

// with unnecessary type annotation

static example_extra_type = fn({n: usize}: {n: usize}) => {
    print(n);
}

// or shorthand

static example_short = fn({n: usize}) => {
    print(n);
}

// and with rename

static example_rename = fn({n as a: usize}) => {
    print(a);
    // print(n); <-- error
}

// and with space for pointless input
static example_pointless = fn({n, ...}: {n: usize, a: string}) => {
    print(n);
}

// opt-in duck typing
static example_allows_extra_fields({n: usize, ...}: {n: usize, ...}) => {
    print(args.n);
}

// duck typing shorthand
static example_duck_typed({n: usize, ...}) => {
    print(n)
}

struct Example {
    n: usize,
    a: string,
}

static takes_partial(e: Example {n, ...}) => {
    print(n);
}

struct ExampleWithPrivate {
    pub(self) n: usize,
    pub(mod) a: string,
} with Self {
    new = fn(n: usize) -> Self { n, ... } => {}
}

// static illegal(e: ExampleWithPrivate { n, ...}) => { <-- type error: ExampleWithPrivate doesn't expose a field n
//     loop {}
// }
```
