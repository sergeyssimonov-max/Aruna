#!/usr/bin/env python3
"""Build a reproducible Aruna installer using Ventura's HFS+ and UDZO tools.

SOURCE_DATE_EPOCH is the Unix timestamp of the release commit. The source must
be the finished, signed universal Aruna.app. The destination must be new.
"""

import argparse
import hashlib
import os
from pathlib import Path
import plistlib
import shutil
import stat
import struct
import subprocess
import sys
import tempfile
import unicodedata
import uuid
from typing import NamedTuple


LAYOUT = Path(__file__).with_name("dmg-finder-layout.bin")
LAYOUT_SHA256 = "2189c8af2e434fd45e400ae73827f920e6532456b3f0a0a9c635f566ce9ed86a"
HFS_EPOCH_DELTA = 2082844800
# With ownership enabled, both stock Ventura installers have this owner/group.
# The installer is read-only. Only public file and directory modes are accepted.
VOLUME_UID = 501
VOLUME_GID = 20


def require(value, message):
    if not value:
        raise ValueError(message)


def run(*args):
    return subprocess.check_output(args)


def timestamp():
    value = os.environ.get("SOURCE_DATE_EPOCH", "")
    require(value.isascii() and value.isdecimal(),
            "SOURCE_DATE_EPOCH must be the nonnegative Unix timestamp of the release commit")
    seconds = int(value) + HFS_EPOCH_DELTA
    require(seconds <= 0xffffffff, "SOURCE_DATE_EPOCH exceeds the HFS+ timestamp range")
    return struct.pack(">I", seconds)


class Entry(NamedTuple):
    mode: int
    flags: int
    finder_info: bytes
    contents: bytes


def digest(path):
    result = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            result.update(chunk)
    return result.digest()


def snapshot(root, installer=False, mounted=False):
    """Walk with lstat; unreadable directories and unsupported metadata are errors."""
    entries = {}
    pending = [root]
    while pending:
        path = pending.pop()
        key = unicodedata.normalize("NFC", str(path.relative_to(root)))
        require(key not in entries, "duplicate normalized path: " + key)
        info = path.lstat()
        if mounted:
            require((info.st_uid, info.st_gid) == (VOLUME_UID, VOLUME_GID), "unexpected volume ownership: " + key)
        # ls -b escapes control characters in names. Additional -e lines are ACLs,
        # which makehybrid cannot faithfully reproduce. Never flatten an input ACL.
        require(len(run("/bin/ls", "-ldeb", path).splitlines()) == 1, "unsupported ACL: " + key)
        attrs = set(run("/usr/bin/xattr", "-s", path).splitlines())
        unknown = attrs - {b"com.apple.provenance", b"com.apple.FinderInfo"}
        require(not unknown, "unsupported extended attributes on {}: {}".format(key, sorted(unknown)))
        finder = bytes(32)
        if b"com.apple.FinderInfo" in attrs:
            finder = bytes.fromhex(run("/usr/bin/xattr", "-spx", "com.apple.FinderInfo", path).decode())
        require(len(finder) == 32, "invalid FinderInfo: " + key)
        expected_flags = stat.UF_HIDDEN if installer and key == ".DS_Store" else 0
        require(info.st_flags == expected_flags, "unsupported BSD flags on {}: {:#x}".format(key, info.st_flags))
        mode = stat.S_IMODE(info.st_mode)
        if stat.S_ISDIR(info.st_mode):
            require(mode == 0o755, "unsupported directory mode: " + key)
            pending.extend(sorted(path.iterdir()))
            contents = b""
        elif stat.S_ISREG(info.st_mode):
            require(info.st_nlink == 1 and mode in (0o644, 0o755), "unsupported hard link or file mode: " + key)
            contents = digest(path)
        else:
            require(installer and key == "Applications" and stat.S_ISLNK(info.st_mode)
                    and mode == 0o755 and os.readlink(path) == "/Applications", "unsupported special file: " + key)
            contents = b"/Applications"
        entries[key] = Entry(info.st_mode, info.st_flags, finder, contents)
    return entries


def catalog_records(image, catalog_start, catalog_length):
    """Return CNID -> (parent, name, record offset) for a makehybrid catalog."""
    def u16(offset):
        return struct.unpack_from(">H", image, offset)[0]

    def u32(offset):
        return struct.unpack_from(">I", image, offset)[0]

    require(catalog_length >= 64 and image[catalog_start + 8] == 1, "invalid catalog header")
    node_size = u16(catalog_start + 32)
    total = u32(catalog_start + 36)
    require(node_size >= 512 and node_size & (node_size - 1) == 0 and node_size * total == catalog_length,
            "unsupported catalog B-tree")
    records = {}
    for node in range(catalog_start, catalog_start + catalog_length, node_size):
        node_kind = image[node + 8]
        require(node_kind in (0, 1, 2, 255), "unknown catalog node type")
        if node_kind != 255:  # header, index, and map nodes contain no file metadata
            continue
        count = u16(node + 10)
        require(2 * (count + 1) <= node_size - 14, "catalog offset table exceeds node")
        offsets = [u16(node + node_size - 2 * (i + 1)) for i in range(count + 1)]
        require(offsets[0] == 14 and offsets == sorted(set(offsets)) and offsets[-1] <= node_size - 2 * len(offsets),
                "invalid catalog record offsets")
        for begin, end in zip(offsets, offsets[1:]):
            key, limit = node + begin, node + end
            require(key + 8 <= limit, "truncated catalog key")
            record_offset = key + 2 + u16(key)
            require(record_offset + 2 <= limit and record_offset == key + 8 + 2 * u16(key + 6), "invalid catalog key")
            kind = u16(record_offset)
            if kind in (3, 4):  # folder/file thread; contains no dates or permissions
                require(record_offset + 10 <= limit and record_offset + 10 + 2 * u16(record_offset + 8) == limit
                        and u16(record_offset + 2) == 0 and u16(key + 6) == 0, "unknown catalog thread")
                continue
            require(kind in (1, 2) and record_offset + (88 if kind == 1 else 248) == limit, "unknown catalog record")
            require(u16(record_offset + 2) == (0 if kind == 1 else 2) and u32(record_offset + 44) == 0,
                    "unexpected catalog flags, link count or device")
            cnid = u32(record_offset + 8)
            parent = u32(key + 2)
            name = unicodedata.normalize("NFC", image[key + 8:record_offset].decode("utf-16-be"))
            require(cnid not in records and name not in ("", ".", "..") and "/" not in name, "invalid catalog name or ID")
            records[cnid] = (parent, name, record_offset)
    return records


def catalog_path(cnid, records):
    components = []
    visited = set()
    while cnid != 2:  # HFS+ root folder ID
        require(cnid in records and cnid not in visited, "invalid catalog parent chain")
        visited.add(cnid)
        cnid, name, _ = records[cnid]
        components.append(name)
    return "/".join(reversed(components)) or "."


def canonicalize_hfs(path, source, date):
    """Only modify our private makehybrid image. Offsets follow Apple TN1150."""
    image = bytearray(path.read_bytes())
    def u32(offset):
        return struct.unpack_from(">I", image, offset)[0]

    # Driver descriptor, one partition-map entry, and a single Apple_HFS entry.
    require(len(image) >= 1536 and image[:2] == b"ER" and image[512:514] == b"PM"
            and u32(516) == 2 and image[1024:1026] == b"PM" and u32(1028) == 2
            and image[1072:1104].rstrip(b"\0") == b"Apple_HFS", "unsupported makehybrid partition map")
    partition_start, size = u32(1032) * 512, u32(1036) * 512
    header_offset, backup_header_offset = partition_start + 1024, partition_start + size - 1024
    require(0 < partition_start < header_offset < backup_header_offset < partition_start + size <= len(image),
            "invalid HFS+ partition bounds")
    for position in (header_offset, backup_header_offset):
        require(image[position:position + 4] == b"H+\0\x04" and image[position + 8:position + 12] == b"cerd"
                and u32(position + 4) == 0x100 and u32(position + 12) == 0
                and u32(position + 40) * u32(position + 44) == size,
                "expected a clean, unjournaled makehybrid HFS+ volume")
    require(image[header_offset:header_offset + 512] == image[backup_header_offset:backup_header_offset + 512],
            "HFS+ header copies differ")
    block = u32(header_offset + 40)
    require(block >= 512 and block & (block - 1) == 0, "unsupported HFS+ allocation block")
    catalog = header_offset + 272
    catalog_start = partition_start + u32(catalog + 16) * block
    catalog_length = struct.unpack_from(">Q", image, catalog)[0]
    require(u32(catalog + 20) == u32(catalog + 12) and catalog_length <= u32(catalog + 20) * block
            and catalog_start >= header_offset + 512 and catalog_start + catalog_length <= backup_header_offset,
            "unsupported catalog extent layout")
    records = catalog_records(image, catalog_start, catalog_length)
    require(2 in records and records[2][:2] == (1, "Aruna"), "unexpected volume root")
    seen = set()
    for cnid, (_, _, record_offset) in records.items():
        key = catalog_path(cnid, records)
        require(key in source and key not in seen, "unexpected catalog path: " + key)
        seen.add(key)
        entry = source[key]
        created_mode = struct.unpack_from(">H", image, record_offset + 42)[0]
        require(stat.S_IFMT(created_mode) == stat.S_IFMT(entry.mode), "catalog file type mismatch: " + key)
        require(image[record_offset + 40:record_offset + 42] == bytes(2), "unexpected catalog BSD permissions flags: " + key)
        # Preserve Finder metadata and permissions, including the execute bit.
        # makehybrid supplies artificial Finder positions even inside the signed app.
        image[record_offset + 48:record_offset + 80] = entry.finder_info
        struct.pack_into(">II", image, record_offset + 32, VOLUME_UID, VOLUME_GID)
        struct.pack_into(">H", image, record_offset + 42, entry.mode)
        image[record_offset + 12:record_offset + 32] = date * 4 + bytes(4)  # four dates and no backup
    require(seen == source.keys(), "makehybrid omitted a path")
    for position in (header_offset, backup_header_offset):
        image[position + 16:position + 32] = date * 4
        image[position + 104:position + 112] = bytes(8)
    volume = memoryview(image)[partition_start:partition_start + size]
    volume_id = hashlib.sha256(volume).digest()[:8]
    for position in (header_offset, backup_header_offset):
        image[position + 104:position + 112] = volume_id
    path.write_bytes(volume)  # the stock installer is flat HFS+, without an optical partition map


def canonicalize_udif(path):
    image = bytearray(path.read_bytes())
    trailer_offset = len(image) - 512
    require(trailer_offset > 0 and image[trailer_offset:trailer_offset + 4] == b"koly"
            and struct.unpack_from(">III", image, trailer_offset + 4) == (4, 512, 1)
            and struct.unpack_from(">II", image, trailer_offset + 56) == (1, 1),
            "unsupported single-segment UDIF header or flags")
    xml_start, xml_size = struct.unpack_from(">QQ", image, trailer_offset + 216)
    require(0 < xml_start < trailer_offset and xml_start + xml_size == trailer_offset
            and struct.unpack_from(">QQQQQ", image, trailer_offset + 16) == (0, 0, xml_start, 0, 0)
            and image[trailer_offset + 232:trailer_offset + 352] == bytes(120), "unsupported UDIF forks or signature")
    require("blkx" in plistlib.loads(image[xml_start:trailer_offset])["resource-fork"], "missing UDIF block map")
    # The segment UUID is outside the checksummed payload. Derive a standard
    # name-based UUID after zeroing hdiutil's randomly generated value.
    segment = slice(trailer_offset + 64, trailer_offset + 80)
    image[segment] = bytes(16)
    image[segment] = uuid.uuid5(uuid.NAMESPACE_OID, hashlib.sha256(image).hexdigest()).bytes
    path.write_bytes(image)


def verify_app(app):
    with (app / "Contents/Info.plist").open("rb") as source:
        info = plistlib.load(source)
    require(info.get("CFBundleIdentifier") == "com.sergeyssimonov.aruna"
            and info.get("CFBundleExecutable") == "aruna-desktop", "unexpected application identity")
    binary = app / "Contents/MacOS/aruna-desktop"
    require(set(run("/usr/bin/lipo", "-archs", binary).split()) == {b"arm64", b"x86_64"}, "expected a universal application")
    run("/usr/bin/codesign", "--verify", "--all-architectures", "--deep", "--strict", app)


def stage(app, root, original):
    root.mkdir(mode=0o755)
    os.chmod(root, 0o755)
    run("/usr/bin/ditto", "--rsrc", "--extattr", app, root / "Aruna.app")
    require(snapshot(root / "Aruna.app") == original, "staging changed the application")
    shutil.copyfile(LAYOUT, root / ".DS_Store")
    shutil.copyfile(app / "Contents/Resources/icon.icns", root / ".VolumeIcon.icns")
    (root / "Applications").symlink_to("/Applications")
    os.lchmod(root / "Applications", 0o755)
    for name in (".DS_Store", ".VolumeIcon.icns"):
        os.chmod(root / name, 0o644)
    # Flags and type/creator fields were checked on both mounted stock images.
    metadata = [(root, bytes(8) + b"\x04\x00" + bytes(22)),
                (root / ".DS_Store", b" " * 8 + b"\x40\x10" + bytes(22)),
                (root / ".VolumeIcon.icns", bytes(4) + b"icnC" + bytes(24))]
    for path, finder in metadata:
        run("/usr/bin/xattr", "-wx", "com.apple.FinderInfo", finder.hex(), path)
    return snapshot(root, installer=True)


def verify_image(packed, mount, expected):
    info = plistlib.loads(run("/usr/bin/hdiutil", "imageinfo", "-plist", packed))
    props = info["Properties"]
    require(info["Format"] == "UDZO" and props["Checksummed"] and props["Compressed"] and props["Kernel Compatible"]
            and not props["Partitioned"] and not props["Encrypted"] and not props["Software License Agreement"],
            "unexpected UDZO properties")
    run("/usr/bin/hdiutil", "verify", "-quiet", packed)
    mount.mkdir()
    run("/usr/bin/hdiutil", "attach", "-readonly", "-owners", "on", "-nobrowse", "-noautoopen", "-mountpoint", mount, packed)
    try:
        require(mount.is_mount() and os.statvfs(mount).f_flag & os.ST_RDONLY, "image did not mount read-only")
        actual = snapshot(mount, installer=True, mounted=True)
        changed = sorted(k for k in expected.keys() | actual.keys() if expected.get(k) != actual.get(k))
        require(not changed, "mounted contents differ: " + ", ".join(changed[:5]))
        verify_app(mount / "Aruna.app")
    finally:
        run("/usr/bin/hdiutil", "detach", mount)


def package(app, output):
    date = timestamp()
    require(not app.is_symlink() and app.is_dir() and app.name == "Aruna.app", "expected a local signed Aruna.app")
    app = app.resolve(strict=True)
    parent = output.parent.resolve(strict=True)
    require(app != parent and app not in parent.parents, "output must be outside the source application")
    output = parent / output.name
    require(not os.path.lexists(output), "output already exists: " + str(output))
    require(not LAYOUT.is_symlink() and digest(LAYOUT).hex() == LAYOUT_SHA256, "unexpected Finder layout")
    original = snapshot(app)
    verify_app(app)
    work = Path(tempfile.mkdtemp(prefix=".aruna-udzo-", dir=parent))
    mount = work / "volume"
    try:
        root, raw, packed = work / "source", work / "optical.dmg", work / "compressed.dmg"
        expected = stage(app, root, original)
        run("/usr/bin/hdiutil", "makehybrid", "-quiet", "-hfs", "-hfs-volume-name", "Aruna", "-o", raw, root)
        canonicalize_hfs(raw, expected, date)
        run("/usr/bin/hdiutil", "convert", "-quiet", "-tasks", "1", "-format", "UDZO",
            "-imagekey", "zlib-level=9", "-o", packed, raw)
        canonicalize_udif(packed)
        verify_image(packed, mount, expected)
        with packed.open("rb") as source:
            os.fsync(source.fileno())
        os.link(packed, output)  # atomic and exclusive on the destination filesystem
    finally:
        # If detach failed, retain the private directory. Never recurse through a mount.
        if not mount.is_mount():
            shutil.rmtree(work)


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("app", type=Path)
    cli.add_argument("new_dmg", type=Path)
    args = cli.parse_args()
    try:
        package(args.app, args.new_dmg)
    except (OSError, ValueError, KeyError, struct.error, subprocess.CalledProcessError, plistlib.InvalidFileException) as error:
        sys.exit("Aruna installer: " + str(error))
