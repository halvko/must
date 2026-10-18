#!/usr/bin/env bash
# The editor plugins build the grammar from the commit in each pin file's
# `rev`. That commit holds only the grammar sources, with fixed metadata, so
# its hash follows from `src/` alone: a change can pin its own grammar, and
# `publish` tags the commit on main so the editors can fetch it.
set -euo pipefail

grammar=editors/tree-sitter-must/src
pin_files=(editors/zed/extension.toml editors/helix/languages.toml)
rev_re='rev = "([^"]*)"'

# The commit of the grammar sources in the working tree.
rev() {
  local index tree
  index=$(mktemp)
  trap 'rm -f "$index"' RETURN
  GIT_INDEX_FILE=$index git read-tree --empty
  GIT_INDEX_FILE=$index git add -- "$grammar"
  tree=$(GIT_INDEX_FILE=$index git write-tree)
  GIT_AUTHOR_NAME=must GIT_AUTHOR_EMAIL=must@users.noreply.github.com \
    GIT_AUTHOR_DATE='1970-01-01T00:00:00Z' \
    GIT_COMMITTER_NAME=must GIT_COMMITTER_EMAIL=must@users.noreply.github.com \
    GIT_COMMITTER_DATE='1970-01-01T00:00:00Z' \
    git -c commit.gpgSign=false commit-tree "$tree" -m "The tree-sitter-must grammar sources"
}

pin_line() { # file
  grep -n -m1 -E "(^|[ {])$rev_re" "$1" | cut -d: -f1
}

pin_of() { # file
  sed -nE "$(pin_line "$1")s/.*$rev_re.*/\\1/p" "$1"
}

check() {
  local want file failed=0
  want=$(rev)
  for file in "${pin_files[@]}"; do
    if [ "$(pin_of "$file")" != "$want" ]; then
      echo "::error file=$file,line=$(pin_line "$file")::The grammar pin does not match" \
        "the grammar sources in $grammar. Pin $want, or run .github/scripts/grammar-pin.sh pin."
      failed=1
    fi
  done
  return "$failed"
}

pin() {
  local want file
  want=$(rev)
  for file in "${pin_files[@]}"; do
    sed -i -E "$(pin_line "$file")s/$rev_re/rev = \"$want\"/" "$file"
  done
}

# Pushes the tag `grammar/<rev>` that keeps the pinned commit fetchable.
publish() {
  local want
  want=$(rev)
  git push origin "$want:refs/tags/grammar/$want"
}

case ${1-} in
  check | pin | publish) "$1" ;;
  *) echo "usage: $0 check | pin | publish" >&2; exit 2 ;;
esac
