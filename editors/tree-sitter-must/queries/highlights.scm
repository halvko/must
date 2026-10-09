; Highlights for Must, in Helix scope names. This is the canonical query:
; `crates/tree-sitter-agreement` checks it against the server's semantic
; highlighting and derives Zed's copy from it. A capture claims only what
; syntax decides; where two patterns capture one node the later one wins,
; so general patterns come first.

; ---- comments and literals ------------------------------------------------

(line_comment) @comment.line
(block_comment) @comment.block

(string) @string
(escape_sequence) @constant.character.escape
(char) @constant.character
(integer) @constant.numeric.integer
(boolean) @constant.builtin.boolean

; ---- keywords -------------------------------------------------------------

["if" "else" "match"] @keyword.control.conditional
["loop" "break"] @keyword.control.repeat
(continue_expr) @keyword.control.repeat
"return" @keyword.control.return
"fn" @keyword.function
["static" "const" "type" "struct" "enum" "trait" "let"] @keyword.storage.type
["mut" "pub" "raw" "unsafe" "extern"] @keyword.storage.modifier
["as" "with" "only" "impl" "for" "requires"] @keyword

; ---- operators and punctuation --------------------------------------------

; `<` and `>` are operators in a generic list too: the semantic layer
; classifies the token, not its role.
["+" "-" "*" "/" "==" "!=" "<" ">" "<=" ">=" "=" "&" "->" "=>"] @operator

["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," ";" ":" "::" "." "..."] @punctuation.delimiter
(rest_pat) @punctuation.delimiter

(region) @label
(never_type) @type.builtin

; ---- declarations ---------------------------------------------------------

(static_item name: (identifier) @variable)
(static_item name: (identifier) @function value: (fn_literal))
(type_item name: (identifier) @type)
(trait_item name: (identifier) @type.interface)
(member name: (identifier) @function)
(type_param name: (identifier) @type.parameter)
(const_param name: (identifier) @type.parameter)
(enum_variant name: (identifier) @type.enum.variant)
(record_type_field name: (identifier) @variable.other.member)

; ---- bindings -------------------------------------------------------------

; A name bound anywhere inside a parameter's pattern is a `param_name`.
(bind_pat (identifier) @variable)
(bind_pat (param_name) @variable.parameter)
(variant_pat binding: (identifier) @variable)

; With a rename the first name is the field, not a binding.
(record_pat_field name: (identifier) @variable !rename)
(record_pat_field name: (param_name) @variable.parameter)
(record_pat_field name: (identifier) @variable.other.member rename: (_))
(record_pat_field rename: (identifier) @variable)
(record_pat_field rename: (param_name) @variable.parameter)

; ---- names in type position -----------------------------------------------

(path_type head: (identifier) @type)
(path_type variant: (identifier) @type.enum.variant)
(with_clause name: (identifier) @type)
(requires_clause name: (identifier) @type)
(named_arg name: (identifier) @type)
(newtype_pat type: (identifier) @type)
(variant_pat type: (identifier) @type)
(variant_pat variant: (identifier) @type.enum.variant)
(elided_variant_expr variant: (identifier) @type.enum.variant)

; ---- names in expression position -----------------------------------------

; A callee stays a variable: it can be a parameter, a local or a type's
; constructor, and only resolution tells.
(path_expr head: (identifier) @variable)
(path_expr segment: (identifier) @variable)
(field_expr field: (identifier) @variable.other.member)
(record_expr_field name: (identifier) @variable.other.member)
