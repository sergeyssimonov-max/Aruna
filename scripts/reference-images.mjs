#!/usr/bin/env node
// The reference images in cli/releases/, checked against what records them:
// the gate of tree cleanliness (specification 6.8, layer 1).
//
//   node scripts/reference-images.mjs --check
//   node scripts/reference-images.mjs --check --github-digest <sha256>
//   node scripts/reference-images.mjs --check --tag v2.6.3 --root <dir>
//
// Two things, both offline.
//
// Every pair. Each `Aruna_<version>-macos-universal.dmg` in cli/releases/ has
// its `Aruna_<version>-SHA256SUMS.txt` beside it, and the image's SHA-256 is
// the line of that file and the `dmg_sha256` of the entry `v<version>` in
// .github/reference-release.json. No image without an entry, no checksum file
// without its image.
//
// The last published tag. Owner's decision of 2026-10-11, variant (B) of the
// question the task pre-3.0.0-hardening put: the rule of 6.7 stays – the image
// in the tree is the one downloaded back from the release, which the release
// job builds from the tag itself, so it lands in the commit after the tag, as
// it did in 2.6.3 – and the gate checks that this was done: the reference
// record has an entry for the last published tag, its copy is in
// cli/releases/, and the copy's SHA-256 is the record's and the digest of the
// release asset on GitHub.
//
// The last published tag is the highest version among the tags `v*` of the
// repository, unless `--tag` names it. GitHub's digest is not fetched: this
// runs without the network. It is `--github-digest` when given – the value of
//   gh release view <tag> --json assets --jq '.assets[] | select(.name == "Aruna-macos-universal.dmg") | .digest'
// without its `sha256:` – and otherwise the record's, and then the comparison
// with GitHub is the manual line of 6.7 that this prints.
//
// Exit 0 when everything holds, 1 with every problem named when something
// does not, 2 when the arguments are wrong.

import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const IMAGE = /^Aruna_(\d+\.\d+\.\d+)-macos-universal\.dmg$/;
const SUMS = /^Aruna_(\d+\.\d+\.\d+)-SHA256SUMS\.txt$/;
const HEX = /^[0-9a-f]{64}$/;
/** The name the release publishes its image under, and its checksum file names. */
const PUBLISHED = "Aruna-macos-universal.dmg";

const sha256 = (path) =>
  createHash("sha256").update(readFileSync(path)).digest("hex");

/** `[major, minor, patch]` of `v1.2.3`, or null. */
function version(tag) {
  const m = /^v(\d+)\.(\d+)\.(\d+)$/.exec(tag);
  return m ? m.slice(1).map(Number) : null;
}

/** The highest version among `tags`, or null when none is a version. */
export function lastTag(tags) {
  const order = (a, b) => {
    const [x, y] = [version(a), version(b)];
    return x[0] - y[0] || x[1] - y[1] || x[2] - y[2];
  };
  return tags.filter(version).sort(order).at(-1) ?? null;
}

/**
 * Everything wrong with the reference images of `root`, as sentences; an
 * empty list when all holds. `tag` is the last published tag, `github` the
 * digest of its release asset, or undefined to take the record's.
 */
export function problems(root, tag, github) {
  const out = [];
  const record = JSON.parse(
    readFileSync(join(root, ".github", "reference-release.json"), "utf8"),
  );
  const entries = new Map(record.references.map((r) => [r.tag, r]));
  const dir = join(root, "cli", "releases");
  const names = readdirSync(dir);

  for (const name of names) {
    const sums = SUMS.exec(name);
    if (sums && !names.includes(`Aruna_${sums[1]}-macos-universal.dmg`)) {
      out.push(`${name} has no image beside it`);
    }
    const image = IMAGE.exec(name);
    if (!image) continue;
    const v = image[1];
    const digest = sha256(join(dir, name));
    const sumsName = `Aruna_${v}-SHA256SUMS.txt`;
    if (!names.includes(sumsName)) {
      out.push(`${name} has no ${sumsName} beside it`);
    } else {
      const line = readFileSync(join(dir, sumsName), "utf8")
        .split("\n")
        .map((l) => l.trim().split(/\s+\*?/))
        .find((parts) => parts[1] === PUBLISHED);
      if (!line) out.push(`${sumsName} names no ${PUBLISHED}`);
      else if (line[0] !== digest)
        out.push(`${name} is ${digest}, ${sumsName} says ${line[0]}`);
    }
    const entry = entries.get(`v${v}`);
    if (!entry)
      out.push(`${name} is in the tree, and the reference record has no v${v}`);
    else if (entry.dmg_sha256 !== digest)
      out.push(
        `${name} is ${digest}, the reference record says ${entry.dmg_sha256}`,
      );
  }

  const entry = entries.get(tag);
  if (!entry) {
    out.push(
      `the last published tag ${tag} has no entry in the reference record`,
    );
    return out;
  }
  const copy = `Aruna_${tag.slice(1)}-macos-universal.dmg`;
  if (!names.includes(copy)) {
    out.push(
      `the last published tag ${tag} has no copy of its image in cli/releases/`,
    );
    return out;
  }
  const digest = sha256(join(dir, copy));
  const published = github ?? entry.dmg_sha256;
  if (digest !== published) {
    out.push(
      `${copy} is ${digest}, the release asset of ${tag} on GitHub is ${published}`,
    );
  }
  if (published !== entry.dmg_sha256) {
    out.push(
      `the release asset of ${tag} on GitHub is ${published}, the reference record says ${entry.dmg_sha256}`,
    );
  }
  return out;
}

function main(argv) {
  const arg = (name) => {
    const i = argv.indexOf(name);
    return i < 0 ? undefined : argv[i + 1];
  };
  if (!argv.includes("--check")) {
    console.error(
      "usage: reference-images.mjs --check [--tag vX.Y.Z] [--github-digest <sha256>] [--root <dir>]",
    );
    return 2;
  }
  const root =
    arg("--root") ?? join(dirname(fileURLToPath(import.meta.url)), "..");
  const github = arg("--github-digest")?.replace(/^sha256:/, "");
  if (github !== undefined && !HEX.test(github)) {
    console.error(`--github-digest is not a SHA-256: ${github}`);
    return 2;
  }
  const tag =
    arg("--tag") ??
    lastTag(
      execFileSync("git", ["-C", root, "tag", "--list", "v*"], {
        encoding: "utf8",
      }).split("\n"),
    );
  if (!tag || !version(tag) || !existsSync(join(root, "cli", "releases"))) {
    console.error(`no published tag to check: ${tag}`);
    return 2;
  }
  const found = problems(root, tag, github);
  for (const p of found) console.error(p);
  if (found.length) return 1;
  console.log(`reference images hold; the last published tag is ${tag}`);
  if (github === undefined) {
    console.log(
      `GitHub's digest was not given: compare by hand (6.7) –\n` +
        `  gh release view ${tag} --json assets --jq '.assets[] | select(.name == "${PUBLISHED}") | .digest'`,
    );
  }
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url))
  process.exit(main(process.argv.slice(2)));
