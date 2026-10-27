# CLAUDE.md

Read `prd.md`, `design.md` and `plan.md` before changing anything; work only inside the slice you were given.

## Git

- Never add a Co-Authored-By trailer to any commit message.
- Work on a branch and open a draft PR into `main`; never push to `main`.

## Guardrails

- Run `scripts/check.sh` before every push; CI (`.github/workflows/ci.yml`) runs the same script and must be green before review.
- Hooks in `.githooks/` block pushes to `main` and Co-Authored-By trailers. The owner enables them once per clone with `git config core.hooksPath .githooks`; agents never edit git config.
- Never weaken a gate to get green: no edits to `scripts/check.sh`, `.githooks/` or `.github/` unless that is your slice, no `--no-verify`, no `#[allow(...)]`/`@ts-ignore` added to silence a check, no `Cargo.lock`/`package-lock.json` churn outside a slice that needs new dependencies.
