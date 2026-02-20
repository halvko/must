= Must lang

Consistent function/closure syntax:
```
static main: fn() -> () = fn() -> () {
    let s: &'static str = "hello";
    (fn () -> () print(s))();
}
```
or with less annotation:
```
static main = fn {
    let s = "hello";
    (fn print(s))();
}
```
and ofc one could do bs like this:
```
static main = fn {
    let s = "hello";
    (fn print)()(s);
}
```

```
static example = fn (arg: fn -> usize) {
    arg();
}

static main = fn {
    example(fn 42 + 69)
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

== Statics and consts are accessible in their declarations

```must
static fib = (n: usize) => {
    match n {
        0 | 1 => n,
        _ => fib(n-1) + fib(n-2),
    }
}

const fib2 = (n: usize) => {
    match n {
        0 | 1 => n,
        _ => fib2(n-1) + fib2(n-2),
    }
}
```

== Statics and consts can be mutually recursive
```must
const fib1 = fn (n: usize) => {
    if (n == 0 || n == 1) {
        n
    } else {
        fib2(n-1) + fib2(n-2)
    }
}

static fib2 = fn (n: usize) => {
    match n {
        0 | 1 => n,
        _ => fib1(n-1) + fib1(n-2),
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
