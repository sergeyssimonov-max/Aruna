#!/usr/bin/env bash
# Build release binary + Aruna.dmg + zip. Run from repo: bash cli/scripts/make_release.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
cargo build --release
STAGE="$ROOT/stage/Aruna"
REL="$ROOT/releases"
rm -rf "$STAGE"
mkdir -p "$STAGE" "$REL"
cp target/release/aruna "$STAGE/aruna"
chmod +x "$STAGE/aruna"
cp icon.svg README.md "$STAGE/"
cat > "$STAGE/INSTALL.txt" <<'EOT'
Aruna — TLHdig inventory generator

  chmod +x ./aruna
  ./aruna

Downloads Zenodo TLHdig, parses XML, writes HTML to ~/Downloads.
macOS Universal .app: run ./build_app.sh on macOS 13+.
EOT
# pycdlib required (CI: pip install --user pycdlib)
if ! python3 -c "import pycdlib" 2>/dev/null; then
  pip install --user pycdlib
  export PATH="$HOME/.local/bin:$PATH"
fi
python3 "$ROOT/scripts/pack_dmg.py"
ls -lah "$REL"
