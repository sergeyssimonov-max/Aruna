#!/usr/bin/env python3
"""Ventura integration test; pass a signed universal Aruna.app and optionally a second clean build."""

import argparse
import hashlib
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile


PACKER = Path(__file__).with_name("build-repro-dmg.py")
LAYOUT = PACKER.with_name("dmg-finder-layout.bin")


def check_filesystem(image):
    attachment = plistlib.loads(subprocess.check_output(
        ["/usr/bin/hdiutil", "attach", "-nomount", "-readonly", "-plist", image]
    ))
    devices = [row for row in attachment["system-entities"] if row.get("dev-entry")]
    disk = next(row["dev-entry"] for row in devices if re.fullmatch(r"/dev/disk\d+", row["dev-entry"]))
    try:
        if len(devices) != 1 or devices[0].get("volume-kind") != "hfs":
            raise RuntimeError("expected a single flat HFS+ volume")
        subprocess.run(["/sbin/fsck_hfs", "-fn", disk.replace("/dev/disk", "/dev/rdisk", 1)], check=True)
    finally:
        subprocess.run(["/usr/bin/hdiutil", "detach", disk], check=True)


def expect_refusal(packer, app, output, env, reason):
    result = subprocess.run([sys.executable, "-IB", packer, app, output], env=env, capture_output=True, text=True)
    if result.returncode == 0 or reason not in result.stderr:
        raise RuntimeError("missing expected refusal {!r}: {}".format(reason, result.stderr))


def test(first, second):
    # Test constant, independent of wall time; real release builds use the commit timestamp.
    env = {**os.environ, "SOURCE_DATE_EPOCH": "1795391287"}
    with tempfile.TemporaryDirectory(prefix="aruna-repro-dmg-test-") as temp:
        work = Path(temp)
        images = [work / "a.dmg", work / "b.dmg"]
        for app, image in zip((first, second), images):
            subprocess.run([sys.executable, "-IB", PACKER, app, image], check=True, env=env)
            check_filesystem(image)
        subprocess.run(["/usr/bin/cmp", *images], check=True)
        print("Two complete DMGs match: " + hashlib.sha256(images[0].read_bytes()).hexdigest(), flush=True)

        occupied = work / "occupied.dmg"
        sentinel = b"existing output"
        occupied.write_bytes(sentinel)
        expect_refusal(PACKER, first, occupied, env, "output already exists")
        if occupied.read_bytes() != sentinel:
            raise RuntimeError("the pre-existing output changed")

        modified = work / PACKER.name
        shutil.copyfile(PACKER, modified)
        (work / LAYOUT.name).write_bytes(LAYOUT.read_bytes() + b"wrong layout")
        rejected = work / "rejected.dmg"
        expect_refusal(modified, first, rejected, env, "unexpected Finder layout")
        for value in (None, "-1", "2212122496"):
            invalid = env.copy()
            if value is None:
                del invalid["SOURCE_DATE_EPOCH"]
            else:
                invalid["SOURCE_DATE_EPOCH"] = value
            expect_refusal(PACKER, first, rejected, invalid, "SOURCE_DATE_EPOCH")
        if os.path.lexists(rejected):
            raise RuntimeError("invalid input created an output")
        print("Occupied output, changed Finder layout, missing and out-of-range timestamps rejected.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("app", type=Path)
    parser.add_argument("second_app", nargs="?", type=Path)
    args = parser.parse_args()
    test(args.app, args.second_app or args.app)
