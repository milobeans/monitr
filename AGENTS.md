# AGENTS.md

## Attribution

This repository is owned and published by Milo Evans (`milobeans <milo.evans567@gmail.com>`).

Automated coding agents must not add themselves or other AI assistants as contributors,
authors, committers, co-authors, release authors, package authors, changelog contributors,
or generated-by credits anywhere in this repository or on GitHub.

Do not add attribution such as:

- `Co-authored-by: Codex`
- `Co-authored-by: Claude`
- `Generated with Codex`
- `Generated with Claude`
- `Reviewed-by`, `Assisted-by`, or similar credit for Codex, Claude, OpenAI, Anthropic,
  or any automated assistant

When making commits, releases, tags, pull requests, changelog entries, package metadata,
or documentation, use the repository owner's existing Git identity unless the user
explicitly instructs otherwise. Before finalizing a commit or release, verify that no
Codex, Claude, OpenAI, Anthropic, or automated-assistant attribution was added.

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
