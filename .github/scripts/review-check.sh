#!/usr/bin/env bash
# Checks a reviewer's output files in the current directory and prints one
# line per problem, so the reviewer can be asked to fix them. Prints nothing
# when they are fine. Needs GH_TOKEN, PR and REPO.
set -uo pipefail

if [ ! -s review-summary.md ]; then
  echo "review-summary.md is missing or empty."
fi

if [ ! -f review-findings.json ]; then
  echo "review-findings.json is missing; write [] if you have no findings."
  exit 0
fi
if ! jq -e 'type == "array"' review-findings.json > /dev/null 2>&1; then
  echo "review-findings.json is not a JSON array."
  exit 0
fi

# Every line a review comment can go on: the head side of each hunk. If the
# diff cannot be read, lines are not checked. Both lists go to jq as files: a
# large diff is past the limit for one argument.
lines=$(mktemp)
threads=$(mktemp)
echo null > "$lines"
echo null > "$threads"
if diff=$(gh pr diff "$PR" --repo "$REPO"); then
  printf '%s\n' "$diff" | awk '
    /^diff --git / { hdr = 1; file = ""; next }
    hdr && /^\+\+\+ / { file = substr($0, 5); sub(/^b\//, "", file); next }
    /^@@ / { hdr = 0; match($0, /\+[0-9]+/); n = substr($0, RSTART + 1, RLENGTH - 1) + 0; next }
    hdr || file == "/dev/null" { next }
    /^[ +]/ { print file ":" n; n++ }' | jq -Rn '[inputs]' > "$lines"
fi
if ids=$(gh api --paginate "repos/$REPO/pulls/$PR/comments" --jq '.[].id'); then
  printf '%s\n' "$ids" | jq -Rn '[inputs | select(length > 0) | tonumber]' > "$threads"
fi

jq -r --slurpfile lines "$lines" --slurpfile threads "$threads" '
  $lines[0] as $lines | $threads[0] as $threads
  | to_entries[] | .key as $i | .value as $f
  | "Entry \($i) in review-findings.json " as $at
  | if ($f | type) != "object" or ($f.body | type) != "string" or $f.body == "" then
      $at + "needs an object with a non-empty string \"body\"."
    elif $f | has("reply_to") then
      if $threads != null and (any($threads[]; . == $f.reply_to) | not) then
        $at + "replies to \($f.reply_to | tojson), which is not the id of an inline comment on this PR."
      else empty end
    elif ($f.path | type) != "string" or ($f.line | type) != "number" then
      $at + "needs a string \"path\" and a number \"line\", or a number \"reply_to\"."
    elif $lines != null and (any($lines[]; . == "\($f.path):\($f.line)") | not) then
      $at + "is on \($f.path):\($f.line), which the diff does not show; GitHub only takes comments on lines the diff shows."
    else empty end' review-findings.json
