#!/usr/bin/env bash
# Sets the `review gate` commit status on a PR's current head. It is red while
# a reviewer's inline thread is unresolved, or while the head has changes no
# reviewer has seen; yellow while a round is coming. It never weighs what the
# reviewers said, only whether every thread was settled.
# Needs GH_TOKEN, REPO, PR and RUN (a link to this run), and a clone of the
# repository.
set -euo pipefail

context="review gate"
pull=$(gh api "repos/$REPO/pulls/$PR")
[ "$(jq -r .state <<< "$pull")" = open ] || exit 0
sha=$(jq -r .head.sha <<< "$pull")
short=${sha:0:7}
base_ref=$(jq -r .base.ref <<< "$pull")
default=$(gh api "repos/$REPO" --jq .default_branch)

post() {
  gh api -X POST "repos/$REPO/statuses/$sha" -f context="$context" \
    -f state="$1" -f description="${2:0:140}" -f target_url="$3" > /dev/null
}

if [ "$(jq -r .draft <<< "$pull")" = true ]; then
  post pending "A draft; the reviewers start once it is ready for review." "$RUN"
  exit 0
fi

. "$(dirname "$0")/review-seen.sh"
markers=$(review_markers "$REPO" "$PR")
reviewed=$(printf '%s\n' "$markers" | last_reviewed)
git fetch -q origin "refs/pull/$PR/head" "$base_ref" "$default"
# A change that keeps the reviewed patch (a rebase) gets no round, so it
# counts as seen.
seen=false
if [ "$reviewed" = "$sha" ]; then
  seen=true
elif [ -n "$reviewed" ] && git fetch -q origin "$reviewed" 2>/dev/null \
  && same_patch "origin/$base_ref" "$reviewed" "$sha"; then
  seen=true
fi

# Unresolved inline threads a reviewer opened; outdated ones count too, since
# only someone reading the fix can say it settled the thread.
open=$(gh api graphql --paginate -F owner="${REPO%/*}" -F name="${REPO#*/}" -F pr="$PR" -f query='
  query($owner: String!, $name: String!, $pr: Int!, $endCursor: String) {
    repository(owner: $owner, name: $name) {
      pullRequest(number: $pr) {
        reviewThreads(first: 100, after: $endCursor) {
          pageInfo { hasNextPage endCursor }
          nodes {
            isResolved
            comments(first: 1) { nodes { url body author { __typename login } } }
          }
        }
      }
    }
  }' --jq '.data.repository.pullRequest.reviewThreads.nodes[]
    | select(.isResolved | not) | .comments.nodes[0]
    | select(.author.__typename == "Bot" and .author.login == "github-actions"
      and (.body | test("^\\*\\*[a-z-]+:\\*\\*")))
    | .url')
threads=$(printf '%s' "$open" | grep -c . || true)

state=success
url=$RUN
parts=()
if [ "$seen" = false ]; then
  running=$(gh api "repos/$REPO/actions/workflows/review.yml/runs?head_sha=$sha" \
    --jq '[.workflow_runs[] | select(.status != "completed")] | length')
  state=failure
  # The Review run's end sets this status again.
  if [ "$running" -gt 0 ]; then
    state=pending
    parts+=("The reviewers have not finished with $short.")
  # GitHub runs no pull_request workflow on a PR that does not merge.
  elif [ "$(jq -r .mergeable <<< "$pull")" = false ]; then
    parts+=("The PR conflicts with $base_ref, so no reviewer can run; resolve the conflict.")
  else
    # A Review run checks out the PR merged into its base.
    running_copy=$sha
    git fetch -q origin "refs/pull/$PR/merge" 2>/dev/null && running_copy=$(git rev-parse FETCH_HEAD)
    case $(workflow_verdict "origin/$default" "$running_copy") in
      stale) parts+=("The review workflow here is older than $default's, so no reviewer can run; rebase onto $default.") ;;
      edited) parts+=("This PR or its base edits the review workflow, so no reviewer can run on it; it needs a human review.") ;;
      *) parts+=("No reviewer has seen $short; add the \`review\` label for a round.") ;;
    esac
  fi
fi
if [ "$threads" -gt 0 ]; then
  [ "$state" = pending ] || state=failure
  url=$(head -n1 <<< "$open")
  if [ "$threads" -eq 1 ]; then
    parts+=("1 reviewer thread is unresolved.")
  else
    parts+=("$threads reviewer threads are unresolved.")
  fi
fi
[ "$state" = success ] && parts+=("The reviewers have seen $short and every thread of theirs is resolved.")
post "$state" "${parts[*]}" "$url"
