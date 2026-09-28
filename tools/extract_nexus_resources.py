"""Carve embedded resources (firmware .ufw, device JSON) out of the Nexus .NET Native DLL.

.NET Native (UWP) drops the CLR metadata, so dnfile cannot read it. The resources survive as
a name table (UTF-8 name, then two packed varints: offset, length) and one contiguous blob
section. Offsets are relative to the first resource, which is unsupported_devices.json.

Usage: python3 tools/extract_nexus_resources.py [DLL] [OUT_DIR]
"""

import json
import re
import sys
from pathlib import Path

PREFIX = b"HJC.GameSir.Nexus2_0."
NAME = re.compile(re.escape(PREFIX) + rb"[\x20-\x7e]+?\.(?:ufw|json)")


def varint(b: bytes, i: int) -> tuple[int, int]:
    """Native metadata packed uint: low bits of the first byte give the length."""
    x = b[i]
    if x & 1 == 0:
        return x >> 1, i + 1
    if x & 3 == 1:
        return (x >> 2) | (b[i + 1] << 6), i + 2
    if x & 7 == 3:
        return (x >> 3) | (b[i + 1] << 5) | (b[i + 2] << 13), i + 3
    if x & 15 == 7:
        return (x >> 4) | (b[i + 1] << 4) | (b[i + 2] << 12) | (b[i + 3] << 20), i + 4
    return int.from_bytes(b[i + 1 : i + 5], "little"), i + 5


def main() -> None:
    dll = Path(
        sys.argv[1]
        if len(sys.argv) > 1
        else "private/nexus-app/HJC.GameSir.Nexus2_0.dll"
    )
    out = Path(sys.argv[2] if len(sys.argv) > 2 else "private/firmware")
    data = dll.read_bytes()
    entries = []
    for m in NAME.finditer(data):
        offset, i = varint(data, m.end())
        length, _ = varint(data, i)
        entries.append((m.group()[len(PREFIX) :].decode(), offset, length))
    if not entries:
        sys.exit(f"no {PREFIX.decode()}* resource names in {dll}")
    first = min(entries, key=lambda e: e[1])
    base = None
    for m in re.finditer(rb"[\[{]", data):
        try:
            json.loads(data[m.start() : m.start() + first[2]])
        except ValueError:
            continue
        base = m.start()
        break
    if base is None:
        sys.exit(f"blob base not found (first resource {first[0]})")
    print(f"blob base {base}, {len(entries)} resources")
    for name, offset, length in entries:
        parts = name.split(".")
        target = out.joinpath(*parts[:-2], f"{parts[-2]}.{parts[-1]}")
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data[base + offset : base + offset + length])
        print(f"{length:>9}  {target}")


if __name__ == "__main__":
    main()
