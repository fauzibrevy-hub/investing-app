**Slice:** <!-- L1 / L2 / L3a … from plan.md §2 — one slice per PR; branch slice/<id>-<slug> cut from current main -->
**Source commit (copy PRs):** <!-- finance-app commit, plan.md §1 -->
**Unanswered owner questions:** <!-- none, or: question → default built (PR stays draft) -->

- [ ] Stayed inside the slice; canon (`prd.md`, `design.md`, `plan.md`) unchanged or the change is the point of this PR
- [ ] Commits as Menganto via env vars, no Co-Authored-By trailer, hooks active (`core.hooksPath=.githooks`)
- [ ] `scripts/check.sh` passes locally and CI is green
- [ ] plan.md §4 checks for this slice that CI does not run: run locally, output pasted below
- [ ] Screenshots (UI slices) committed under `docs/qa/<slice>/` and linked here
- [ ] `Cargo.lock` / `web/package-lock.json` unchanged, or this slice creates or prunes dependencies
- [ ] No live data touched (`~/finance-app-data`, `~/investing-app-data`, `:8787`, `:8789`, `:8445`)

### Check output
