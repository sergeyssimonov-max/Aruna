/**
 * Every path a README points at must exist.
 *
 * The root README is the first thing anyone reads, and it had drifted into
 * naming three things that were no longer true — among them a link to
 * `public/data/`, a directory that had moved to `src/data/` some twenty commits
 * earlier. A link to a directory that is not there is the one kind of staleness
 * a machine can catch, so it should be caught by a machine rather than by a
 * reader who follows it and finds a 404.
 *
 * Only repository-relative links are checked. What a sentence claims about the
 * code is beyond this test; what it points at is not.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { resolve, dirname, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");

/** The READMEs a contributor is likely to open, and where each one's links resolve from. */
const DOCS = ["README.md", "cli/README.md", "PERFORMANCE.md"];

/** `[text](target)` — the target only. */
const LINK = /\[[^\]]*\]\(([^)]+)\)/g;

/** Whether a link points somewhere in this repository. */
function isRepoPath(target) {
  if (/^(https?:|mailto:|#)/.test(target)) return false;
  // A bare fragment or a query is not a path.
  return !target.startsWith("#");
}

for (const doc of DOCS) {
  test(`every path ${doc} links to exists`, () => {
    const text = readFileSync(resolve(root, doc), "utf8");
    const from = dirname(resolve(root, doc));

    const missing = [];
    for (const [, target] of text.matchAll(LINK)) {
      if (!isRepoPath(target)) continue;
      // Strip a fragment: `PERFORMANCE.md#rules` is a link to the file.
      const path = target.split("#")[0];
      if (path === "") continue;
      if (!existsSync(resolve(from, path))) missing.push(target);
    }

    assert.deepEqual(
      missing,
      [],
      `${doc} links to ${missing.join(", ")}, which is not in the repository`,
    );
  });
}

/**
 * A shell command in a README is an instruction, and the reader runs it from
 * wherever the surrounding block put them.
 *
 * The root README told anyone packaging a release to `bash scripts/make_release.sh`
 * from the repository root, where there is no such file — both scripts live in
 * `cli/`, which is where CI runs them from and where the block had forgotten to
 * send the reader. The link check above could not see it: a command is not a
 * link, and `npm run` was the only command being checked.
 *
 * Each fenced block is read in order, so a `cd` inside it moves the directory
 * the commands after it are resolved against, exactly as it would for a reader
 * working down the page.
 */
test("every script the READMEs tell the reader to run is there to run", () => {
  /** `bash path/to.sh`, `sh path/to.sh`, `./path/to.sh` — the path only. */
  const INVOCATION = /^\s*(?:(?:bash|sh)\s+|\.\/)([\w./-]+\.sh)\b/;
  const missing = [];

  for (const doc of DOCS) {
    const text = readFileSync(resolve(root, doc), "utf8");
    for (const [, block] of text.matchAll(/```bash\n([\s\S]*?)```/g)) {
      // Where the block has left the reader standing.
      let cwd = dirname(resolve(root, doc));
      for (const line of block.split("\n")) {
        const cd = /^\s*cd\s+([\w./-]+)/.exec(line);
        if (cd) {
          cwd = resolve(cwd, cd[1]);
          continue;
        }
        const run = INVOCATION.exec(line);
        if (run && !existsSync(resolve(cwd, run[1]))) {
          missing.push(`${doc}: ${run[1]} (from ${relative(root, cwd) || "."})`);
        }
      }
    }
  }

  assert.deepEqual([...new Set(missing)], [], "a README tells the reader to run a script that is not there");
});

/**
 * The command a reader is told to run has to be one that exists.
 *
 * `npm run <name>` in a README is an instruction, and an instruction naming a
 * script that was renamed fails in front of whoever followed it.
 */
test("every npm script the READMEs name is defined", () => {
  const defined = new Set(Object.keys(JSON.parse(readFileSync(resolve(root, "package.json"), "utf8")).scripts));
  const missing = new Set();

  for (const doc of DOCS) {
    const text = readFileSync(resolve(root, doc), "utf8");
    for (const [, name] of text.matchAll(/npm run ([\w:-]+)/g)) {
      if (!defined.has(name)) missing.add(`${doc}: npm run ${name}`);
    }
  }

  assert.deepEqual([...missing], [], "a README tells the reader to run a script that is not there");
});
