; Indents for Must, in Helix's capture names: an `@indent` node puts the
; lines after its first one level in, an `@outdent` token takes a level off
; the line it starts. `crates/tree-sitter-agreement` checks that every
; corpus line is indented where this query puts it.

; ---- bracketed bodies -----------------------------------------------------

; Every node that owns a bracket pair, the hidden `_element_block` of
; `with`, `unsafe` and `for` included.
[
  (block_expr)
  (match_expr)
  (record_type)
  (record_expr)
  (record_pat)
  (enum_type)
  (requires_def)
  (with_group)
  (impl_element)
  (unsafe_element)
  (for_element)
  (param_list)
  (arg_list)
  (enum_variant)
  (newtype_pat)
  (variant_pat)
  (paren_expr)
  (array_type)
  (array_expr)
  (index_expr)
  (generic_param_list)
  (generic_arg_list)
] @indent

["}" ")" "]"] @outdent

; Only a generic list's `>`: elsewhere it is the comparison operator.
(generic_param_list ">" @outdent)
(generic_arg_list ">" @outdent)

; ---- continuation lines ---------------------------------------------------

; A wrapped operand or field access.
[
  (bin_expr)
  (field_expr)
] @indent

; A value on a later line than its `=` (an arm's `=>`) goes in a level; one
; on the same line (`let x = if c {`) is indented by its own brackets, and
; a second level would stack on the `else` arm. The capture is the whole
; statement because Helix 25.07 indents a node's own first line only under
; `"scope" "all"`, a setting other Helix releases do not read.
(let_stmt
  "=" @sign
  value: (_) @value
  (#not-same-line? @value @sign)) @indent

(assign_stmt
  "=" @sign
  value: (_) @value
  (#not-same-line? @value @sign)) @indent

(static_item
  "=" @sign
  value: (_) @value
  (#not-same-line? @value @sign)) @indent

(member
  "=" @sign
  value: (_) @value
  (#not-same-line? @value @sign)) @indent

(record_expr_field
  "=" @sign
  value: (_) @value
  (#not-same-line? @value @sign)) @indent

(match_arm
  "=>" @sign
  value: (_) @value
  (#not-same-line? @value @sign)) @indent
