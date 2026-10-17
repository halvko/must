# What counts as reviewed, shared by the Review workflow and the review gate.
# Source it; the functions need a clone of the repository, and gh for
# review_markers.

review_workflow=.github/workflows/review.yml

# The summary markers on a PR, one per reviewer and round, oldest first:
# `<!-- review-round: <role> <sha> <label|auto>[ findings] -->`. Fails when
# the comments cannot be read.
review_markers() { # repo pr
  local bodies
  bodies=$(gh api --paginate "repos/$1/issues/$2/comments" \
    --jq '.[] | select(.user.type == "Bot") | .body') || return
  grep -E '^<!-- review-round: [^ ]+ [0-9a-f]{40} (label|auto)( findings)? -->$' <<< "$bodies" || true
}

# The head of the last round in the markers on stdin, or nothing.
last_reviewed() {
  awk 'NF { s = $4 } END { print s }'
}

# Whether two commits carry the same patch against their merge base with
# base, as a rebase leaves it. Both commits must be fetched.
same_patch() { # base reviewed head
  patch_of() {
    git diff "$(git merge-base "$1" "$2")" "$2" | git patch-id --stable | cut -d' ' -f1
  }
  [ "$(patch_of "$1" "$2")" = "$(patch_of "$1" "$3")" ]
}

# Why the reviewers cannot run on a commit, the PR merged into its base as a
# Review run checks it out: empty when its review workflow matches
# default's; else `edited` when it changes the workflow since it left
# default, and `stale` when it only lags behind.
workflow_verdict() { # default running
  git diff --quiet "$1" "$2" -- "$review_workflow" && return 0
  if git diff --quiet "$(git merge-base "$1" "$2")" "$2" -- "$review_workflow"; then
    echo stale
  else
    echo edited
  fi
}
