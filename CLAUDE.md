# Impulse — project rules for Claude

## Working with the user

- When scope, placement, or intent is ambiguous, ask — one precise question beats a wrong
  assumption. Never resolve an open decision by picking silently; if a question is pending,
  no downstream action that depends on its answer.

## Git policy (hard rules)

- Never commit without the user reviewing the changes first. Finish work in the working
  tree, verify it builds, present a diff summary, and wait for explicit approval.
- Never push anything to GitHub — branches, tags, forced updates, release operations —
  without explicit permission for that specific push. Prior pushes grant nothing.

## Tooling

- pnpm only, never npm. Pre-commit hook (`.githooks`, `core.hooksPath`) runs oxlint,
  `oxfmt --check`, and `cargo fmt --check`.
- oxfmt: tabs, double quotes (`.oxfmtrc.json`, `.editorconfig`, tab-width 2).
