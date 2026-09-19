/**
 * Tree-sitter grammar for Must. It follows `crates/syntax/src/grammar.rs`
 * production by production; node names are the rowan kinds in snake_case.
 * Separators (`;`, and `,` after a match arm) are optional, and the
 * conflicts that creates resolve toward the longer expression, as the
 * rowan parser's greedy expression rule does.
 */

/// <reference types="tree-sitter-cli/dsl" />
// @ts-check

const PREC = {
  compare: 1,
  additive: 2,
  multiplicative: 3,
  unary: 4,
  postfix: 5,
};

/** `rule` separated by `,`, with an optional trailing `,`. */
function commaSep(rule) {
  return optional(seq(rule, repeat(seq(',', rule)), optional(',')));
}

/**
 * The binding patterns under the rule names `names`, a bound name being
 * `bound`. A parameter's patterns are a second copy whose bound names are
 * `param_name` nodes, so a query tells a parameter at any nesting depth.
 */
function bindingPatterns(names, bound) {
  const node = ($, kind) =>
    names[kind] === kind ? $[kind] : alias($[names[kind]], $[kind]);
  return {
    [names.pattern]: $ => choice(
      node($, 'bind_pat'),
      node($, 'record_pat'),
      node($, 'newtype_pat'),
    ),

    [names.bind_pat]: $ => bound($),

    [names.record_pat]: $ => seq(
      'struct',
      '{',
      commaSep(choice(node($, 'record_pat_field'), $.rest_pat)),
      '}',
    ),

    // With a rename the first name selects a field and binds nothing.
    [names.record_pat_field]: $ => seq(
      optional('mut'),
      choice(
        field('name', bound($)),
        seq(field('name', $._name), 'as', field('rename', bound($))),
      ),
    ),

    [names.newtype_pat]: $ => prec(1, seq(
      field('type', $.identifier),
      '(',
      $[names.pattern],
      ')',
    )),
  };
}

/** `rule` joined by `+`: bounds, region joins, capability ladders. */
function plusSep(rule) {
  return seq(rule, repeat(seq('+', rule)));
}

module.exports = grammar({
  name: 'must',

  externals: $ => [$.block_comment, $.string_content, $._error_sentinel],

  extras: $ => [/\s/, $.line_comment, $.block_comment],

  word: $ => $.identifier,

  rules: {
    source_file: $ => repeat($._item),

    // ---- items ----------------------------------------------------------

    _item: $ => choice($.static_item, $.type_item, $.trait_item),

    static_item: $ => seq(
      optional('extern'),
      choice('static', 'const'),
      field('name', $._name),
      optional(seq(':', field('type', $._type))),
      optional(seq('=', field('value', $._expr))),
      repeat($._trailing_clause),
      optional(';'),
    ),

    type_item: $ => seq(
      'type',
      field('name', $._name),
      '=',
      field('value', choice($.enum_type, $._type)),
      repeat($._trailing_clause),
      optional(';'),
    ),

    trait_item: $ => seq(
      'trait',
      field('name', $._name),
      '=',
      field('value', choice($.requires_def, $.trait_alias)),
      repeat($._trailing_clause),
      optional(';'),
    ),

    _name: $ => choice($.identifier, '_'),

    _trailing_clause: $ => choice($.with_group, $.only_clause),

    only_clause: $ => seq('only', plusSep(field('capability', $.identifier))),

    // ---- traits and `with` chains ---------------------------------------

    trait_alias: $ => plusSep($._type),

    requires_def: $ => seq(
      optional('unsafe'),
      'requires',
      optional($.generic_param_list),
      commaSep($.requires_clause),
      '{',
      repeat(choice($.member, ';')),
      '}',
    ),

    requires_clause: $ => seq(
      field('name', $.identifier),
      ':',
      plusSep($._type),
    ),

    with_group: $ => seq(
      'with',
      optional($.generic_param_list),
      commaSep($.with_clause),
      $._element_block,
    ),

    with_clause: $ => choice(
      seq($.region, ':', plusSep($.region)),
      seq(field('name', $.identifier), ':', plusSep($._type)),
      seq(field('name', $.identifier), '=', $._type),
    ),

    _element_block: $ => seq('{', repeat(choice($._element, ';')), '}'),

    _element: $ => choice($.impl_element, $.unsafe_element, $.for_element),

    impl_element: $ => seq(
      'impl',
      field('head', $._type),
      choice(seq('{', repeat(choice($.member, ';')), '}'), ';'),
    ),

    unsafe_element: $ => seq('unsafe', choice($._element, $._element_block)),

    for_element: $ => seq(
      'for',
      field('type', $._type),
      choice($._element, $._element_block),
    ),

    member: $ => prec.right(seq(
      optional(choice('type', 'const')),
      field('name', $._name),
      optional(seq(':', field('type', $._type))),
      optional(seq('=', field('value', $._expr))),
      optional(';'),
    )),

    // ---- generics -------------------------------------------------------

    generic_param_list: $ => seq(
      '::',
      '<',
      commaSep(choice($.region_param, $.const_param, $.type_param)),
      '>',
    ),

    region_param: $ => seq(
      $.region,
      optional(seq(':', plusSep($.region))),
    ),

    const_param: $ => seq(
      'const',
      field('name', $._name),
      ':',
      field('type', $._type),
    ),

    type_param: $ => seq(
      field('name', $._name),
      optional(seq(':', plusSep($._type))),
    ),

    _turbofish: $ => seq('::', $.generic_arg_list),

    generic_arg_list: $ => seq(
      '<',
      commaSep(choice($.named_arg, $.region_arg, $.const_arg, $.type_arg)),
      '>',
    ),

    named_arg: $ => seq(field('name', $.identifier), '=', $._type),

    region_arg: $ => plusSep($.region),

    const_arg: $ => choice(
      $._literal,
      $.const_block_expr,
      seq('const', choice($._literal, alias($._bare_path, $.path_expr))),
    ),

    type_arg: $ => $._type,

    _bare_path: $ => field('head', $.identifier),

    // ---- types ----------------------------------------------------------

    _type: $ => choice(
      $.unit_type,
      $.never_type,
      $.hole_type,
      $.path_type,
      $.fn_type,
      $.record_type,
      $.array_type,
      $.borrow_type,
      $.raw_ptr_type,
    ),

    unit_type: _ => seq('(', ')'),

    never_type: _ => '!',

    hole_type: _ => '_',

    path_type: $ => prec.right(seq(
      field('head', $.identifier),
      optional(choice(
        $._turbofish,
        seq('::', field('variant', $.identifier)),
        $._retired_bare_angle_args,
      )),
    )),

    fn_type: $ => prec.right(seq(
      optional('unsafe'),
      'fn',
      optional($.generic_param_list),
      alias($.fn_type_param_list, $.param_list),
      optional($.ret_type),
    )),

    fn_type_param_list: $ => seq(
      '(',
      commaSep(alias($.fn_type_param, $.param)),
      ')',
    ),

    fn_type_param: $ => choice(
      seq(
        optional('mut'),
        field('pattern', $._param_pattern),
        ':',
        field('type', $._type),
      ),
      field('type', $._type),
    ),

    ret_type: $ => seq('->', $._type),

    record_type: $ => seq(
      'struct',
      optional($.generic_param_list),
      '{',
      commaSep(choice($.record_type_field, '...')),
      '}',
    ),

    record_type_field: $ => seq(
      optional('pub'),
      field('name', $.identifier),
      ':',
      field('type', $._type),
    ),

    enum_type: $ => seq(
      'enum',
      optional($.generic_param_list),
      '{',
      commaSep($.enum_variant),
      '}',
    ),

    enum_variant: $ => seq(
      field('name', $.identifier),
      optional(seq('(', commaSep($._type), ')')),
    ),

    array_type: $ => seq(
      '[',
      field('element', $._type),
      ';',
      field('length', alias($._array_length, $.const_arg)),
      ']',
    ),

    _array_length: $ => choice(
      $._literal,
      alias($._bare_path, $.path_expr),
      $.const_block_expr,
      seq('const', choice($._literal, alias($._bare_path, $.path_expr))),
    ),

    borrow_type: $ => choice(
      prec.right(PREC.postfix, seq($._type, '.', '&', optional('mut'), optional($._turbofish))),
      $._retired_prefix_borrow_type,
    ),

    raw_ptr_type: $ => choice(
      prec.right(PREC.postfix, seq($._type, '.', '&', 'raw', optional('mut'))),
      $._retired_prefix_raw_ptr_type,
    ),

    // ---- statements -----------------------------------------------------

    block_expr: $ => seq(
      '{',
      repeat(choice($.let_stmt, $.assign_stmt, $.expr_stmt, ';')),
      '}',
    ),

    let_stmt: $ => prec.right(seq(
      'let',
      optional('mut'),
      field('pattern', $._binding_pattern),
      optional(seq(':', field('type', $._type))),
      optional(seq('=', field('value', $._expr))),
      optional(';'),
    )),

    assign_stmt: $ => prec.right(seq(
      field('target', $._expr),
      '=',
      field('value', $._expr),
      optional(';'),
    )),

    expr_stmt: $ => prec.right(seq($._expr, optional(';'))),

    // ---- patterns -------------------------------------------------------

    ...bindingPatterns({
      pattern: '_binding_pattern',
      bind_pat: 'bind_pat',
      record_pat: 'record_pat',
      record_pat_field: 'record_pat_field',
      newtype_pat: 'newtype_pat',
    }, $ => $._name),

    ...bindingPatterns({
      pattern: '_param_pattern',
      bind_pat: '_param_bind_pat',
      record_pat: '_param_record_pat',
      record_pat_field: '_param_record_pat_field',
      newtype_pat: '_param_newtype_pat',
    }, $ => choice(alias($.identifier, $.param_name), '_')),

    rest_pat: _ => '..',

    _match_pattern: $ => choice(
      $.literal_pat,
      $.wildcard_pat,
      $.rest_pat,
      $.variant_pat,
      alias($._match_bind_pat, $.bind_pat),
    ),

    literal_pat: $ => $._literal,

    wildcard_pat: _ => '_',

    _match_bind_pat: $ => $.identifier,

    variant_pat: $ => choice(
      seq(
        optional(field('type', $.identifier)),
        '::',
        field('variant', $.identifier),
        optional($._pattern_binding_list),
      ),
      $._retired_unqualified_variant_pat,
    ),

    _pattern_binding_list: $ => seq(
      '(',
      commaSep(choice(field('binding', $._name), $.rest_pat)),
      ')',
    ),

    // ---- expressions ----------------------------------------------------

    _expr: $ => choice(
      $._literal,
      $.path_expr,
      $.elided_variant_expr,
      $.paren_expr,
      $.block_expr,
      $.const_block_expr,
      $.unsafe_block_expr,
      $.array_expr,
      $.record_expr,
      $.fn_literal,
      $.if_expr,
      $.match_expr,
      $.loop_expr,
      $.break_expr,
      $.continue_expr,
      $.return_expr,
      $.call_expr,
      $.index_expr,
      $.field_expr,
      $.deref_expr,
      $.borrow_expr,
      $.addr_of_expr,
      $.neg_expr,
      $.bin_expr,
    ),

    _literal: $ => choice($.integer, $.string, $.char, $.boolean),

    integer: _ => /[0-9][0-9_]*/,

    boolean: _ => choice('true', 'false'),

    string: $ => seq(
      '"',
      repeat(choice($.string_content, $.escape_sequence)),
      '"',
    ),

    char: $ => seq(
      "'",
      repeat(choice(
        alias(token.immediate(prec(1, /[^'\\\n]+/)), $.char_content),
        $.escape_sequence,
      )),
      token.immediate("'"),
    ),

    escape_sequence: _ => token.immediate(/\\[^\n]/),

    path_expr: $ => prec.right(seq(
      field('head', $.identifier),
      optional(choice(
        seq($._turbofish, optional($._qualified_segment)),
        $._qualified_segment,
      )),
    )),

    _qualified_segment: $ => prec.right(seq(
      '::',
      field('segment', $.identifier),
      optional($.member_generic_args),
    )),

    member_generic_args: $ => $._turbofish,

    elided_variant_expr: $ => seq('::', field('variant', $.identifier)),

    paren_expr: $ => seq('(', $._expr, ')'),

    const_block_expr: $ => seq('const', $.block_expr),

    unsafe_block_expr: $ => seq('unsafe', choice($.block_expr, $.fn_literal)),

    array_expr: $ => seq(
      '[',
      optional(choice(
        seq($._expr, ';', $._expr),
        seq($._expr, repeat(seq(',', $._expr)), optional(',')),
      )),
      ']',
    ),

    record_expr: $ => seq(
      'struct',
      '{',
      commaSep(choice($.record_expr_field, '...')),
      '}',
    ),

    record_expr_field: $ => seq(
      optional('pub'),
      field('name', $.identifier),
      optional(seq(':', choice(
        field('type', $._type),
        $._retired_field_colon_value,
      ))),
      optional(seq('=', field('value', $._expr))),
    ),

    fn_literal: $ => choice(
      seq(
        optional('const'),
        'fn',
        optional($.generic_param_list),
        optional($.param_list),
        optional($.ret_type),
        field('body', $.block_expr),
      ),
      $._retired_extern_fn_literal,
    ),

    param_list: $ => seq('(', commaSep($.param), ')'),

    param: $ => seq(
      optional('mut'),
      field('pattern', $._param_pattern),
      optional(seq(':', field('type', $._type))),
    ),

    if_expr: $ => prec.right(seq(
      'if',
      field('condition', $._expr),
      field('consequence', $.block_expr),
      optional(seq('else', field('alternative', choice($.if_expr, $.block_expr)))),
    )),

    match_expr: $ => seq(
      'match',
      field('scrutinee', $._expr),
      '{',
      repeat($.match_arm),
      '}',
    ),

    match_arm: $ => prec.right(seq(
      field('pattern', $._match_pattern),
      '=>',
      field('value', $._expr),
      optional(','),
    )),

    loop_expr: $ => seq('loop', field('body', $.block_expr)),

    break_expr: $ => prec.right(seq('break', optional($._expr))),

    continue_expr: _ => 'continue',

    return_expr: $ => prec.right(seq('return', optional($._expr))),

    call_expr: $ => prec(PREC.postfix, seq(
      field('callee', $._expr),
      $.arg_list,
    )),

    arg_list: $ => seq('(', commaSep($._expr), ')'),

    index_expr: $ => prec(PREC.postfix, seq(
      field('base', $._expr),
      '[',
      field('index', $._expr),
      ']',
    )),

    field_expr: $ => prec.right(PREC.postfix, seq(
      field('receiver', $._expr),
      '.',
      field('field', $.identifier),
      optional($.member_generic_args),
    )),

    deref_expr: $ => prec(PREC.postfix, seq($._expr, '.', '*')),

    borrow_expr: $ => choice(
      prec.right(PREC.postfix, seq(
        $._expr,
        '.',
        '&',
        optional('mut'),
        optional($._turbofish),
      )),
      $._retired_prefix_borrow_expr,
    ),

    addr_of_expr: $ => choice(
      prec.right(PREC.postfix, seq($._expr, '.', '&', 'raw', optional('mut'))),
      $._retired_prefix_addr_of_expr,
    ),

    neg_expr: $ => prec(PREC.unary, seq('-', $._expr)),

    bin_expr: $ => choice(
      ...[
        [PREC.compare, choice('==', '!=', '<', '>', '<=', '>=')],
        [PREC.additive, choice('+', '-')],
        [PREC.multiplicative, choice('*', '/')],
      ].map(([precedence, operator]) => prec.left(precedence, seq(
        field('left', $._expr),
        field('operator', operator),
        field('right', $._expr),
      ))),
    ),

    // ---- retired spellings ----------------------------------------------
    //
    // Spellings the language no longer has, which the rowan parser still
    // builds real nodes for so validation can offer the rewrite. They are
    // modelled so a file that contains one keeps its highlighting, and
    // they go when the rowan parser stops building nodes for them.

    // `&x`, `&mut x`: now `x.&`, `x.&mut`.
    _retired_prefix_borrow_expr: $ => prec(PREC.unary, seq(
      '&',
      optional('mut'),
      $._expr,
    )),

    // `&raw x`, `&raw mut x`: now `x.&raw`, `x.&raw mut`.
    _retired_prefix_addr_of_expr: $ => prec(PREC.unary, seq(
      '&',
      'raw',
      optional('mut'),
      $._expr,
    )),

    // `&T`, `&mut T`: now `T.&`, `T.&mut`.
    _retired_prefix_borrow_type: $ => prec.right(seq('&', optional('mut'), $._type)),

    // `&raw T`, `&raw mut T`: now `T.&raw`, `T.&raw mut`.
    _retired_prefix_raw_ptr_type: $ => prec.right(seq(
      '&',
      'raw',
      optional('mut'),
      $._type,
    )),

    // `Boxed<usize>` in type position: now `Boxed::<usize>`.
    _retired_bare_angle_args: $ => $.generic_arg_list,

    // `struct { x: 1 }`: now `struct { x = 1 }`.
    _retired_field_colon_value: $ => field('value', choice($._literal, $.neg_expr)),

    // `extern fn(n: usize) -> usize` as a value, body optional: now an
    // `extern static` declaration.
    _retired_extern_fn_literal: $ => prec.right(seq(
      choice(seq('extern', optional('const')), seq('const', 'extern')),
      'fn',
      optional($.generic_param_list),
      optional($.param_list),
      optional($.ret_type),
      optional(field('body', $.block_expr)),
    )),

    // `Circle(r)` as a match pattern: now `::Circle(r)`.
    _retired_unqualified_variant_pat: $ => seq(
      field('variant', $.identifier),
      $._pattern_binding_list,
    ),

    // ---- tokens ---------------------------------------------------------

    identifier: _ => /_[\p{Alphabetic}\p{N}_]+|\p{Alphabetic}[\p{Alphabetic}\p{N}_]*/,

    region: _ => /@[\p{Alphabetic}_][\p{Alphabetic}\p{N}_]*/,

    line_comment: _ => token(seq('//', /[^\n]*/)),
  },
});
