import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

/** Once per run (a failed test restarts the worker, so a `beforeAll` would truncate again): empty results.tsv. */
export default function globalSetup() {
  const dir = process.env.LIVE_SHOTS_DIR;
  if (!dir) throw new Error("LIVE_SHOTS_DIR is not set (see docs/qa/l4/README-live.md)");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "results.tsv"), "");
}
