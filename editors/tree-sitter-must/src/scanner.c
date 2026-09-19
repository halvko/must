// External scanner for the two tokens a regular lexer cannot produce.
//
// `block_comment` nests (`/* a /* b */ c */` is one comment), and nesting
// depth is not regular. `string_content` is external only so that this
// scanner, which runs before the generated lexer, sees that a string is
// open and does not read a `/*` inside it as a comment.

#include "tree_sitter/parser.h"

#include <wctype.h>

enum TokenType {
  BLOCK_COMMENT,
  STRING_CONTENT,
  ERROR_SENTINEL,
};

// The scanner keeps no state between tokens.
void *tree_sitter_must_external_scanner_create(void) { return NULL; }

void tree_sitter_must_external_scanner_destroy(void *payload) { (void)payload; }

unsigned tree_sitter_must_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload;
  (void)buffer;
  return 0;
}

void tree_sitter_must_external_scanner_deserialize(void *payload, const char *buffer,
                                                   unsigned length) {
  (void)payload;
  (void)buffer;
  (void)length;
}

// Everything up to the closing quote or the next backslash escape.
static bool scan_string_content(TSLexer *lexer) {
  bool has_content = false;
  while (!lexer->eof(lexer) && lexer->lookahead != '"' && lexer->lookahead != '\\') {
    lexer->advance(lexer, false);
    has_content = true;
  }
  lexer->result_symbol = STRING_CONTENT;
  return has_content;
}

// A comment left open at the end of the file runs to the end of the file.
static bool scan_block_comment(TSLexer *lexer) {
  while (iswspace(lexer->lookahead)) {
    lexer->advance(lexer, true);
  }
  if (lexer->lookahead != '/') {
    return false;
  }
  lexer->advance(lexer, false);
  if (lexer->lookahead != '*') {
    return false;
  }
  lexer->advance(lexer, false);

  unsigned depth = 1;
  while (depth > 0 && !lexer->eof(lexer)) {
    int32_t c = lexer->lookahead;
    lexer->advance(lexer, false);
    if (c == '/' && lexer->lookahead == '*') {
      lexer->advance(lexer, false);
      depth++;
    } else if (c == '*' && lexer->lookahead == '/') {
      lexer->advance(lexer, false);
      depth--;
    }
  }
  lexer->result_symbol = BLOCK_COMMENT;
  return true;
}

bool tree_sitter_must_external_scanner_scan(void *payload, TSLexer *lexer,
                                            const bool *valid_symbols) {
  (void)payload;
  // During error recovery every symbol is marked valid, so an open string
  // cannot be told from that alone: only comments are scanned then.
  bool recovering = valid_symbols[ERROR_SENTINEL];
  if (valid_symbols[STRING_CONTENT] && !recovering) {
    return scan_string_content(lexer);
  }
  if (valid_symbols[BLOCK_COMMENT]) {
    return scan_block_comment(lexer);
  }
  return false;
}
