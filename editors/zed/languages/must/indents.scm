; Indents for Must, in Zed's capture names: a line goes one level past the
; line an `@indent` range starts on (its `@start`, else the node's start),
; and a line starting at a range's `@end` back to that range's first line.
; `crates/tree-sitter-agreement` checks that every corpus line is indented
; where this query puts it.

; ---- bracketed bodies -----------------------------------------------------

; Every node that owns a bracket pair, the hidden `_element_block` of
; `with`, `unsafe` and `for` included. The range opens at the bracket so it
; does not merge with a statement range starting at the same place.
(_ "{" @start "}" @end) @indent
(_ "(" @start ")" @end) @indent
(_ "[" @start "]" @end) @indent

; Only a generic list's `<`/`>`: elsewhere they are comparison operators.
(generic_param_list "<" @start ">" @end) @indent
(generic_arg_list "<" @start ">" @end) @indent

; ---- continuation lines ---------------------------------------------------

; A value on the line after its `=` (an arm's `=>`), a wrapped operand or
; field access. Ranges opening on one line indent it once.
[
  (let_stmt)
  (assign_stmt)
  (static_item)
  (member)
  (record_expr_field)
  (match_arm)
  (bin_expr)
  (field_expr)
] @indent
