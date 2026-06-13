# CLAUDE.md

## Attribution

Claude must not add itself, Anthropic, or any AI assistant as a contributor, author,
committer, co-author, release author, changelog contributor, generated-by credit, or
similar attribution in this repository or on GitHub.

Do not add `Co-authored-by`, `Generated with`, `Reviewed-by`, `Assisted-by`, or similar
trailers or notes for Claude, Codex, OpenAI, Anthropic, or automated assistants in commits,
pull requests, releases, changelogs, package metadata, or documentation.

Use Milo Evans' existing repository Git identity (`milobeans <milo.evans567@gmail.com>`)
for repository work unless the user explicitly instructs otherwise. Before finalizing a
commit, tag, release, pull request, or generated release note, verify that no Claude,
Codex, OpenAI, Anthropic, or automated-assistant attribution was added.

Do not rewrite existing commits, tags, releases, or published artifacts to change
historical attribution unless the user explicitly requests that destructive history work.
Use the repository `.mailmap` for non-destructive contributor canonicalization.

## Public Documentation Boundary

Issue tracking, audit notes, implementation plans, release scratchpads, and other internal
maintenance documents are local-only. Do not commit or publish files such as `audit.md`,
`ISSUE.md`, `issue.md`, `ISSUES.md`, `issues.md`, `docs/ISSUE.md`, `docs/issue.md`,
`docs/ISSUES.md`, `docs/issues.md`, `docs/internal/`, `plans/`, `.internal/`, or
`internal/`.

Public-facing documentation must describe shipped user behavior, supported workflows, and
release-relevant facts without exposing internal issue queues or planning notes. If an
internal tracker is needed during a task, keep it in an ignored local path and leave the
working copy available locally rather than adding it to Git.

Before finalizing a commit, tag, release, or package, verify that no internal tracking or
planning document is tracked by Git or included in `cargo package --locked --list`.

## Commit And Release Subjects

Commit, tag, pull request, and release titles must be descriptive user- or maintainer-facing
summaries. Do not use internal issue numbers as titles or title prefixes, for example
`fix issue #3` or `Implement audit findings #2 and #4`. Prefer a concise description of
the behavior changed, such as `Make dead-PID history pruning linear`.
