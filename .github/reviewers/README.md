# Reviewers

Each `*.md` file here (other than this one) is one Claude reviewer. On every
pull request that is opened, reopened or marked ready for review, the
`Review` workflow runs one job per file, in parallel, and that reviewer
leaves inline comments plus a summary comment headed with its name. Adding
the `review` label to a PR asks for a fresh round; the label comes off
again when the round ends.

- To teach a reviewer, edit its file: what to look for, what to ignore,
  examples of past mistakes. The whole file is its brief.
- To add a reviewer, add a file. Its name (without `.md`) is the reviewer's
  name in comments. To retire one, delete it.
- A file may open with front matter choosing its model (`opus`, `sonnet`,
  `haiku`, `fable` or a full model ID); without it the reviewer uses Claude
  Code's default model.
- A brief is plain instructions; the workflow already tells every reviewer
  how to read the PR and how to post, so a brief only says what to judge.
