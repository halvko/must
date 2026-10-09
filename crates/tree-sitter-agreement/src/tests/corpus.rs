//! What the agreement tests run over: every program in `examples/`, plus
//! snippets for the syntax those programs do not use.

use super::repo_root;

pub(super) struct Sample {
    pub(super) name: String,
    pub(super) text: String,
}

pub(super) fn samples() -> Vec<Sample> {
    let dir = repo_root().join("examples");
    let mut samples: Vec<Sample> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "must"))
        .map(|path| Sample {
            name: format!("examples/{}", path.file_name().unwrap().to_string_lossy()),
            text: super::read(&path),
        })
        .collect();
    samples.sort_by(|a, b| a.name.cmp(&b.name));
    assert!(!samples.is_empty(), "no examples in {}", dir.display());
    samples.extend(SNIPPETS.iter().map(|&(name, text)| Sample {
        name: format!("snippet `{name}`"),
        text: text.to_owned(),
    }));
    samples
}

const SNIPPETS: &[(&str, &str)] = &[
    (
        "reserved element and member forms",
        r#"
trait Eq = requires { eq: fn(a: Self, b: Self) -> bool; };
trait Ord = Eq + PartialOrd;
trait Iterator = unsafe requires ::<Item> Self: Eq + Ord, {
    type Item;
    const LEN: usize;
    next: unsafe fn(it: Self) -> Item;
};
type Wrapper = struct::<T> { pub value: T, ... } with ::<U> T: Eq + Ord, U = usize, {
    impl Marker;
    unsafe impl Send {
        send = fn(w: Self) -> () { };
    }
    for T {
        impl Eq {
            eq = fn(a: Self, b: Self) -> bool { true };
        }
        unsafe { impl Sync; }
    }
} only move + copy;
"#,
    ),
    (
        "binding patterns",
        r#"
type Point = struct { x: usize, y: usize };
type Meters = usize;
static norm = fn (Point(struct { x, y as other, .. }): Point, Meters(m): Meters, _: usize) -> usize {
    let struct { mut x, y as renamed } = struct { x = m, y = other };
    let Meters(inner) = Meters(renamed);
    let _ = x;
    x = inner;
    x
};
type Feet = Meters;
type Yards = Feet;
static direct = fn (struct { x, y as other }: Point, mut Yards(Feet(Meters(deep))): Yards) -> usize {
    x + other + deep
};
type Outer = Point;
type Outermost = Outer;
static nested = fn (Outermost(Outer(Point(struct { x as far, y }))): Outermost) -> usize {
    far + y
};
static alias = direct;
"#,
    ),
    (
        "match patterns",
        r#"
type Shape = enum { Circle(usize), Pair(usize, str), Point };
static describe = fn (s: Shape, n: usize, c: char, b: bool) -> usize {
    let a = match s {
        Shape::Circle(r) => r,
        ::Pair(first, _) => first,
        Shape::Point => 0,
    };
    let d = match n {
        0 => 1,
        other => other,
    };
    let e = match c {
        'x' => 1,
        '\n' => 2,
        _ => 3,
    };
    let f = match b {
        true => 1,
        false => 0,
    };
    a + d + e + f
};
"#,
    ),
    (
        "reserved match patterns",
        r#"
type Shape = enum { Pair(usize, str), Point };
static reserved = fn (s: Shape, t: str) -> usize {
    let a = match s {
        ::Pair(first, ..) => first,
        .. => 0,
    };
    match t {
        "one" => a,
        _ => 0,
    }
};
"#,
    ),
    (
        "generic arguments and regions",
        r#"
trait Show = requires { show: fn(s: Self) -> str; } with {
    impl usize { show = fn(s: usize) -> str { "n" }; }
};
type Buf = struct::<const N: usize, T> { data: [T; N] };
type Grid = struct::<const W: usize> { cells: [usize; const W], one: [usize; const { 1 }], two: [usize; const 2] };
static longest = fn::<@a, @b: @a, @c: @a + @b, T: Show>(x: T.&::<@a + @b>, y: T.&mut::<@_>) -> T.&::<@a> { x };
static id = fn::<T>(x: T) -> T { x };
static rep = const fn::<const N: usize>(v: usize, b: Buf::<const N, usize>) -> [usize; N] { [v; N] };
static sized = const fn::<const N: usize, const F: bool>() -> usize { N };
static uses = fn () -> usize {
    let a = Show::<Self = usize>::show(1);
    let b = id::<usize>(2);
    let c = sized::<3, true>();
    let d = sized::<const { 1 + 2 }, false>();
    let e = sized::<const 4, const true>();
    let f: Buf::<2, _> = Buf::<2, usize>(struct { data = [0; 2] });
    let g = b.show::<>();
    b + c + d + e
};
"#,
    ),
    (
        "postfix chains and unary minus",
        r#"
type Cell = struct { n: isize, next: isize.&raw mut };
static chain = fn (mut c: Cell, mut i: isize) -> isize {
    let p = c.&raw mut;
    let q = c.&raw;
    let r = i.&mut;
    let s = i.&;
    unsafe {
        p.*.n = -1;
        let t = p.*.next.*.&raw mut.*;
        let u = p.*.n.&raw.*;
    }
    let arr = [[1, 2], [3, 4]];
    let v: isize = -arr[0][1] - -i * (2 / 1);
    if v <= 0 { v } else if v >= 9 { 9 } else { 0 }
};
"#,
    ),
    (
        "lexical forms",
        r#"
/* a block comment /* nested
   over lines */ still the comment */
static blå_bær = "multi
line \" with \\ escapes \n\t\r\0 and // no comment /* here */";
static quote = '\'';
static backslash = '\\';
static æ = 'æ';
static slash = '/';
static big = 1_000_000;
static _ = true != false;
"#,
    ),
    (
        "loops and jumps",
        r#"
static spin = fn (limit: usize) -> usize {
    let mut i = 0;
    let found = loop {
        i = i + 1;
        if i == 3 { continue; }
        if i > limit { break i; }
        if i == 100 { return 100; }
    };
    loop { break }
    let zeros = [0; 4];
    let none: [usize; 0] = [];
    const { found }
};
static never = fn () -> ! { loop { } };
static apply = fn (f: fn(usize) -> usize, g: unsafe fn(n: usize, _: str) -> (), x: usize) -> usize { f(x) };
static closure = unsafe fn () { };
"#,
    ),
    (
        "type declaration forms",
        r#"
type Meters = usize;
type Ptr = usize.&raw mut;
type Row = [usize; 4];
type Opt = enum::<T> { Some(T), None };
type OnlySome = Opt::<usize>;
type Tagged = struct { which: Opt::Some, row: Row };
type Callback = struct { run: fn(n: usize) -> usize };
"#,
    ),
    (
        "retired prefix borrows",
        r#"
static old = fn (mut x: usize, r: &usize, m: &mut usize, p: &raw mut usize, q: &raw usize) -> () {
    let a = &x;
    let b = &mut x;
    let c = &raw x;
    let d = &raw mut x;
};
"#,
    ),
    (
        "retired field colon value",
        r#"
static old: struct { x: usize, y: isize } = struct { x: 1, y: -2 };
"#,
    ),
    (
        "retired bare angle arguments",
        r#"
type Boxed = struct::<T> { value: T };
static old = fn (b: Boxed<usize>) -> usize { b.value };
"#,
    ),
    (
        "retired extern spellings",
        r#"
static old = extern fn(n: usize) -> usize;
static older = const extern fn();
extern static valued: unsafe fn(n: usize) -> usize = fn (n: usize) -> usize { n };
extern static live: unsafe fn(buf: u8.&raw mut, len: usize) -> isize;
"#,
    ),
    (
        "retired unqualified variant pattern",
        r#"
type Shape = enum { Circle(usize), Point };
static old = fn (s: Shape) -> usize {
    match s {
        Circle(r) => r,
        _ => 0,
    }
};
"#,
    ),
    (
        "unresolved names",
        r#"
static broken = fn () -> usize { missing(nowhere) };
"#,
    ),
];
