# Reviewers

Each `*.md` file here (other than this one) is one Claude reviewer. On every
pull request that is opened, reopened or marked ready for review, the
`Review` workflow runs one job per file, in parallel. A reviewer whose
output is missing or would not post (a finding on a line outside the diff,
say) is told what is wrong and gets one more turn. Once all are done, it
posts their findings as one review of inline comments, and keeps one summary
comment on the PR with a section per reviewer, rewritten in place each round.

Each push that changes the PR's patch starts another round (a rebase
alone does not), as do reopening it and marking it ready, until three
rounds have run (`MAX_ROUNDS` in the workflow); then the summary comment
says the PR needs a human look. Adding the `review` label asks for a round
at any time and starts a fresh budget of three, counting that round; it is
meant for humans, and comes off again when the round ends.

- To teach a reviewer, edit its file: what to look for, what to ignore,
  examples of past mistakes. The whole file is its brief.
- To add a reviewer, add a file. Its name (without `.md`) is the reviewer's
  name in comments. To retire one, delete it.
- A file may open with front matter choosing its model (`opus`, `sonnet`,
  `haiku`, `fable` or a full model ID); without it the reviewer uses Claude
  Code's default model.
- A brief is plain instructions; the workflow already tells every reviewer
  how to read the PR and how to post, so a brief only says what to judge.
