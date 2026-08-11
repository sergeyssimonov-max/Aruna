#!/usr/bin/env python3
"""Pack stage/Aruna into releases/Aruna.dmg (ISO9660+Joliet+RR) and a zip."""
from __future__ import annotations

import hashlib
import zipfile
from pathlib import Path

import pycdlib

ROOT = Path(__file__).resolve().parents[1]
STAGE = ROOT / "stage" / "Aruna"
REL = ROOT / "releases"


def sha256(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


def main() -> None:
    REL.mkdir(parents=True, exist_ok=True)
    files = {p.name: p for p in STAGE.iterdir() if p.is_file()}
    if "aruna" not in files:
        raise SystemExit(f"missing binary in {STAGE}")

    dmg = REL / "Aruna.dmg"
    iso = pycdlib.PyCdlib()
    iso.new(interchange_level=3, joliet=3, rock_ridge="1.09", vol_ident="ARUNA")
    iso.add_directory("/ARUNA", rr_name="Aruna", joliet_path="/Aruna")
    for name, path in sorted(files.items()):
        if "." in name:
            stem, ext = name.rsplit(".", 1)
            iso_name = f"{stem.upper()[:8]}.{ext.upper()[:3]};1"
        else:
            iso_name = f"{name.upper()[:8]};1"
        mode = 0o0100755 if name == "aruna" else 0o0100644
        iso.add_file(
            str(path),
            iso_path=f"/ARUNA/{iso_name}",
            rr_name=name,
            joliet_path=f"/Aruna/{name}",
            file_mode=mode,
        )
    iso.write(str(dmg))
    iso.close()

    zf = REL / "Aruna-linux-x86_64.zip"
    with zipfile.ZipFile(zf, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for name, path in sorted(files.items()):
            z.write(path, arcname=f"Aruna/{name}")

    sums = REL / "SHA256SUMS"
    lines = [f"{sha256(p)}  {p.name}" for p in (dmg, zf)]
    sums.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"wrote {dmg} ({dmg.stat().st_size} bytes)")
    print(f"wrote {zf} ({zf.stat().st_size} bytes)")
    print(sums.read_text(), end="")


if __name__ == "__main__":
    main()
