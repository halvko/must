# Reviewers

Each `*.md` file here (other than this one) is one Claude reviewer. On every
pull request that is opened, reopened or marked ready for review, the
`Review` workflow runs one job per file, in parallel. A reviewer whose
output is missing or would not post (a finding on a line outside the diff,
say) is told what is wrong and gets one more turn. Once all are done, it
posts their findings as one review of inline comments, and keeps one summary
comment on the PR with a section per reviewer, rewritten in place each round.

Each push that changes the PR's patch starts another round (a rebase
alone does not), as does reopening it, until five rounds have run
(`MAX_ROUNDS` in the workflow). Past that, a push still gets a round from
each reviewer whose latest round raised findings, so the change answering
them is reviewed; once none did, the summary comment says the PR needs a
human look. In a later round a reviewer checks that its
earlier findings were fixed, replying in their threads where needed, and
reviews what changed for new problems. Adding the `review` label asks for
a round at any time and starts a fresh budget of five, counting that
round; it is meant for humans, and comes off again when the round ends.
Marking a draft ready for review does the same, since the rounds before it
reviewed a change that was not ready.

The `review gate` status on the PR's head says whether the reviewers are
done with it. It is red while a thread a reviewer opened is unresolved, and
while the head has changes no reviewer has seen: past the round limit, say,
or on a PR that edits the review workflow, which the reviewers cannot run on.
It is yellow while a round is coming and on drafts. It never weighs how
serious a finding is: settle each thread by fixing it or answering why not,
then resolve it. GitHub sends no event when a thread is resolved, so the
gate updates on the next push, reply or round; to update it sooner, re-run
the PR's latest `Review gate` run, or run that workflow for the PR.

- To teach a reviewer, edit its file: what to look for, what to ignore,
  examples of past mistakes. The whole file is its brief.
- To add a reviewer, add a file. Its name (without `.md`) is the reviewer's
  name in comments. To retire one, delete it.
- A file may open with front matter choosing its model (`opus`, `sonnet`,
  `haiku`, `fable` or a full model ID); without it the reviewer uses Claude
  Code's default model.
- A brief is plain instructions; the workflow already tells every reviewer
  how to read the PR and how to post, so a brief only says what to judge.
