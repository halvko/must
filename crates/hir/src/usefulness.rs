//! Pattern usefulness: which `match` arms can run, and which values no arm
//! matches. One algorithm answers both questions for every pattern shape
//! at every depth (Maranget, "Warnings for pattern matching"), so a `let`
//! is just a one-arm `match`.
//!
//! Patterns arrive deconstructed into [`Pat`]s: a constructor applied to
//! sub-patterns, or a wildcard. Types stay with the caller, which answers
//! two questions about them through [`Columns`].

/// A value's outermost shape: what a pattern tests before it looks inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ctor {
    /// An enum variant, by declaration index.
    Variant(u32),
    /// The only shape a type has (an array, a newtype). Matching it tests
    /// nothing.
    Single,
    Char(char),
    /// An integer literal: the magnitude and whether it is negated. `-0`
    /// is normalized to `0`.
    Int {
        value: u128,
        negative: bool,
    },
}

/// A deconstructed pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Pat {
    /// Matches anything: `_`, a binding, a record pattern of binders.
    Wild,
    Ctor(Ctor, Vec<Pat>),
}

/// What the algorithm needs to know about a column's type.
pub(crate) trait Columns {
    type Ty: Clone;
    /// Every constructor a value of `ty` can have, or `None` when there are
    /// too many to list (integers, `char`, anything only `_` matches).
    fn ctors(&mut self, ty: &Self::Ty) -> Option<Vec<Ctor>>;
    /// The types of `ctor`'s fields in a value of `ty`.
    fn fields(&mut self, ty: &Self::Ty, ctor: &Ctor) -> Vec<Self::Ty>;
}

/// The most witnesses [`missing`] reports.
const WITNESS_LIMIT: usize = 8;

/// Whether some value matches `row` and none of `rows`: the arm `row`
/// can run after the arms `rows`.
pub(crate) fn is_useful<C: Columns>(
    cx: &mut C,
    rows: &[Vec<Pat>],
    row: &[Pat],
    tys: &[C::Ty],
) -> bool {
    let Some((head, rest)) = row.split_first() else {
        return rows.is_empty();
    };
    match head {
        Pat::Ctor(ctor, fields) => {
            // A constructor the type cannot have matches no value.
            if cx.ctors(&tys[0]).is_some_and(|all| !all.contains(ctor)) {
                return false;
            }
            let (rows, tys) = specialize(cx, rows, ctor, tys);
            let row: Vec<Pat> = fields.iter().chain(rest).cloned().collect();
            is_useful(cx, &rows, &row, &tys)
        }
        Pat::Wild => match complete_signature(cx, rows, &tys[0]) {
            Some(all) => all.iter().any(|ctor| {
                let (rows, tys) = specialize(cx, rows, ctor, tys);
                let arity = tys.len() + 1 - row.len();
                let row: Vec<Pat> = std::iter::repeat_n(Pat::Wild, arity)
                    .chain(rest.iter().cloned())
                    .collect();
                is_useful(cx, &rows, &row, &tys)
            }),
            None => is_useful(cx, &default_rows(rows), rest, &tys[1..]),
        },
    }
}

/// Values no row matches, as patterns over `tys`: empty when `rows` is
/// exhaustive. At most [`WITNESS_LIMIT`] of them. With no rows at all, the
/// first part lists each variant (`Shape::Circle(_)`, ...) instead of a
/// single `_`.
pub(crate) fn missing<C: Columns>(cx: &mut C, rows: &[Vec<Pat>], tys: &[C::Ty]) -> Vec<Vec<Pat>> {
    missing_in(cx, rows, tys, rows.is_empty())
}

/// [`missing`], naming each shape of the first part when `name_all`.
fn missing_in<C: Columns>(
    cx: &mut C,
    rows: &[Vec<Pat>],
    tys: &[C::Ty],
    name_all: bool,
) -> Vec<Vec<Pat>> {
    let Some((ty, rest_tys)) = tys.split_first() else {
        return if rows.is_empty() {
            vec![Vec::new()]
        } else {
            Vec::new()
        };
    };
    let mut out = Vec::new();
    let heads = head_ctors(rows);
    let all = cx.ctors(ty);
    // Rows that all match anything here miss the same values whatever
    // this part holds, so `_` stands for it. A single-shape part is only
    // opened when a row looks inside it: an array's elements can be many.
    let split = match &all {
        Some(all) => !heads.is_empty() || (name_all && all != &[Ctor::Single]),
        None => false,
    };
    let mut rests: Option<Vec<Vec<Pat>>> = None;
    let push = |out: &mut Vec<Vec<Pat>>, first: Pat, rest: Vec<Pat>| {
        let mut row = vec![first];
        row.extend(rest);
        out.push(row);
        out.len() == WITNESS_LIMIT
    };
    if let (true, Some(all)) = (split, all) {
        for ctor in all {
            let arity = cx.fields(ty, &ctor).len();
            if heads.contains(&ctor) {
                let (rows, tys) = specialize(cx, rows, &ctor, tys);
                for mut witness in missing_in(cx, &rows, &tys, false) {
                    let rest = witness.split_off(arity);
                    if push(&mut out, Pat::Ctor(ctor.clone(), witness), rest) {
                        return out;
                    }
                }
            } else {
                let rests = rests
                    .get_or_insert_with(|| missing_in(cx, &default_rows(rows), rest_tys, false));
                for rest in rests.iter() {
                    let first = Pat::Ctor(ctor.clone(), vec![Pat::Wild; arity]);
                    if push(&mut out, first, rest.clone()) {
                        return out;
                    }
                }
            }
        }
        return out;
    }
    for rest in missing_in(cx, &default_rows(rows), rest_tys, false) {
        if push(&mut out, Pat::Wild, rest) {
            return out;
        }
    }
    out
}

/// Every constructor of `ty` when the rows' first column names all of
/// them, so a wildcard there is split per constructor.
fn complete_signature<C: Columns>(cx: &mut C, rows: &[Vec<Pat>], ty: &C::Ty) -> Option<Vec<Ctor>> {
    let all = cx.ctors(ty)?;
    let heads = head_ctors(rows);
    // A single-shape part counts only when a row looks inside it, so a
    // binder of a long array never splits into its elements.
    let complete = if all == [Ctor::Single] {
        !heads.is_empty()
    } else {
        all.iter().all(|ctor| heads.contains(ctor))
    };
    complete.then_some(all)
}

/// The constructors the rows' first column names, in order.
fn head_ctors(rows: &[Vec<Pat>]) -> Vec<Ctor> {
    let mut heads = Vec::new();
    for row in rows {
        if let Pat::Ctor(ctor, _) = &row[0]
            && !heads.contains(ctor)
        {
            heads.push(ctor.clone());
        }
    }
    heads
}

/// The rows that match a value starting with `ctor`, its fields spliced in
/// place of the first column; and the column types to match.
fn specialize<C: Columns>(
    cx: &mut C,
    rows: &[Vec<Pat>],
    ctor: &Ctor,
    tys: &[C::Ty],
) -> (Vec<Vec<Pat>>, Vec<C::Ty>) {
    let fields = cx.fields(&tys[0], ctor);
    let arity = fields.len();
    let rows = rows
        .iter()
        .filter_map(|row| {
            let head: Vec<Pat> = match &row[0] {
                Pat::Wild => vec![Pat::Wild; arity],
                // A wrong slot count (its own diagnostic) still tests the
                // constructor: missing fields read as `_`, extra ones drop.
                Pat::Ctor(c, sub) if c == ctor => {
                    let mut sub = sub.clone();
                    sub.resize(arity, Pat::Wild);
                    sub
                }
                Pat::Ctor(..) => return None,
            };
            Some(head.into_iter().chain(row[1..].iter().cloned()).collect())
        })
        .collect();
    let tys = fields.into_iter().chain(tys[1..].iter().cloned()).collect();
    (rows, tys)
}

/// The rows whose first column is a wildcard, without it.
fn default_rows(rows: &[Vec<Pat>]) -> Vec<Vec<Pat>> {
    rows.iter()
        .filter(|row| matches!(row[0], Pat::Wild))
        .map(|row| row[1..].to_vec())
        .collect()
}
