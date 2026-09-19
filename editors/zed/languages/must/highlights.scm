; GENERATED from editors/tree-sitter-must/queries/highlights.scm. Do not edit:
; change that file, then run
; `UPDATE_EXPECT=1 cargo test -p tree-sitter-agreement`.

(line_comment) @comment
(block_comment) @comment
(string) @string
(escape_sequence) @string.escape
(char) @string
(integer) @number
(boolean) @boolean
["if" "else" "match"] @keyword
["loop" "break"] @keyword
(continue_expr) @keyword
"return" @keyword
"fn" @keyword
["static" "const" "type" "struct" "enum" "trait" "let"] @keyword
["mut" "pub" "raw" "unsafe" "extern"] @keyword
["as" "with" "only" "impl" "for" "requires"] @keyword
["+" "-" "*" "/" "==" "!=" "<" ">" "<=" ">=" "=" "&" "->" "=>"] @operator
["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," ";" ":" "::" "." "..."] @punctuation.delimiter
(rest_pat) @punctuation.delimiter
(region) @lifetime
(never_type) @type.builtin
(static_item name: (identifier) @variable)
(static_item name: (identifier) @function value: (fn_literal))
(type_item name: (identifier) @type)
(trait_item name: (identifier) @type.interface)
(member name: (identifier) @function)
(type_param name: (identifier) @type)
(const_param name: (identifier) @type)
(enum_variant name: (identifier) @variant)
(record_type_field name: (identifier) @property)
(bind_pat (identifier) @variable)
(bind_pat (param_name) @variable.parameter)
(variant_pat binding: (identifier) @variable)
(record_pat_field name: (identifier) @variable !rename)
(record_pat_field name: (param_name) @variable.parameter)
(record_pat_field name: (identifier) @property rename: (_))
(record_pat_field rename: (identifier) @variable)
(record_pat_field rename: (param_name) @variable.parameter)
(path_type head: (identifier) @type)
(path_type variant: (identifier) @variant)
(with_clause name: (identifier) @type)
(requires_clause name: (identifier) @type)
(named_arg name: (identifier) @type)
(newtype_pat type: (identifier) @type)
(variant_pat type: (identifier) @type)
(variant_pat variant: (identifier) @variant)
(elided_variant_expr variant: (identifier) @variant)
(path_expr head: (identifier) @variable)
(path_expr segment: (identifier) @variable)
(field_expr field: (identifier) @property)
(record_expr_field name: (identifier) @property)
