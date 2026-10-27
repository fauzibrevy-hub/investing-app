# CLAUDE.md

Read `prd.md`, `design.md` and `plan.md` before changing anything; work only inside the slice you were given.

## Git

- Commit as the owner through env vars, never git config: `GIT_AUTHOR_NAME=Menganto GIT_AUTHOR_EMAIL=fauzibrevy@gmail.com GIT_COMMITTER_NAME=Menganto GIT_COMMITTER_EMAIL=fauzibrevy@gmail.com` (decision D17, 2026-10-27, default chosen by Hermes, owner may override in PR review).
- Never add a Co-Authored-By trailer to any commit message.
- Branch `slice/<id>-<slug>` (e.g. `slice/l1-backend-move`), cut from current `main`. Every PR is a draft into `main`; no stacked bases. Never push to `main`.
- Screenshots go under `docs/qa/<slice>/` and are linked in the PR body.
- An owner question nobody answered: build the documented default, keep the PR draft, and say so in the PR body.

## Guardrails

- Run `scripts/check.sh` before every push; CI (`.github/workflows/ci.yml`) runs the same script (hygiene, backend, web, e2e) and must be green before review.
- CI green is necessary, not sufficient. plan.md §4 checks CI can't run (the L1 release-binary smoke, real-device and sandbox checks, screenshots) are run locally and their output pasted into the PR under "Check output".
- Hooks in `.githooks/` block pushes to `main`, Co-Authored-By trailers, and pushed commits that fail `scripts/check.sh hygiene`. GitHub can't protect `main` on this plan, so the pre-push hook is the only push block. Hooks must be active in every clone and agent worktree: the owner sets `git config core.hooksPath .githooks` once per clone; where that is unset, agents run every git command as `git -c core.hooksPath=.githooks …`. check.sh warns when hooks are off. Agents never edit git config.
- `.claude/settings.json` denies pushes to `main`, `--no-verify` and `git config` for Claude Code. omp has no equivalent setting, so for omp lanes the pre-push hook and CI are the only backstop.
- Hygiene (decision D1): `127.0.0.1:8789` and `~/investing-app-data` appear only in `src/config.rs` (with its unit tests), `deploy/`, `scripts/{parity,counts,writes}.sh` and `web/vite.config.ts`; finance-app's `finance-app-data` and `127.0.0.1:8787` only in `scripts/`; `/Users/<name>` nowhere outside `*.md`. No tracked `.env`/`*.env`/`.envrc`, keys, tokens, database files, non-ASCII paths, or files over 1 MB (except `web/public/icons/`).
- The Rust crate lives at the repo root (`Cargo.toml`, `Cargo.lock`) and the web app in `web/` (`web/package.json`); check.sh fails on Rust code or `web/` files without them. `tests/fixtures/` is shared: the web Vitest suites import `tests/fixtures/research/`, so no slice prunes it to what its own tests need.
- Never weaken a gate to get green: no edits to `scripts/check.sh`, `.githooks/`, `.github/` or `.claude/` unless that is your slice, no `--no-verify`, no `#[allow(...)]`/`@ts-ignore` added to silence a check. `Cargo.lock`/`package-lock.json` change only in a slice that creates or prunes dependencies (L1 creates `Cargo.lock`, L2 `web/package-lock.json`); everywhere else they stay untouched.
