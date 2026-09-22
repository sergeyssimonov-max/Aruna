#!/bin/bash
# Coverage of both crates, held to floors: `pnpm coverage` runs this.
#
# **Why floors exist now.** Until 2026-09-22 coverage was measured and recorded
# — `cargo llvm-cov nextest`, «отчет сформирован» — and nothing failed when it
# fell. The acceptance audit of 21.09 found no threshold anywhere in the tree.
#
# **Which run they bind to: the ordinary one, without the heavy `#[ignore]`
# tests.** That run needs no corpus archive, so it gives the same numbers on any
# machine; the heavy run lifts the shell by fourteen points (65 → 80 % of
# regions) and would make a floor that fails wherever the archive is absent.
#
# **The floors are the measurement of 2026-09-22, rounded down, less about a
# point** (docs/PROJECT-SPEC.ru.md, 3.3). Measured: core 95.91 % regions,
# 97.13 % functions, 95.61 % lines; shell 65.39 %, 62.63 %, 66.52 %. They are
# a ratchet, not a target: raise them when the numbers rise, and a change that
# pushes one under its floor is a change that removed tests or added untested
# code — say which in the commit, do not lower the floor to let it through.
#
# Each crate is measured alone (`-p`), because a workspace total lets the
# 18 767 regions of the core hide the shell's 1 303. `clean` first: without it
# the tool is not idempotent (3.6).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd -P)
cd "$root"

cargo llvm-cov clean --workspace

cargo llvm-cov nextest --locked -p aruna --no-tests=pass --summary-only \
  --fail-under-regions 95 --fail-under-functions 96 --fail-under-lines 95

cargo llvm-cov clean --workspace

cargo llvm-cov nextest --locked -p aruna-desktop --no-tests=pass --summary-only \
  --fail-under-regions 64 --fail-under-functions 62 --fail-under-lines 65
