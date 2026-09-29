#!/usr/bin/env python3
"""Fail when a file that git would commit is a firmware image or is larger than 1 MB.

The tool repository must not contain GameSir firmware (docs/architecture.md, Q4). The check
reads the files that git tracks and the files that git does not ignore.
"""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FIRMWARE_SUFFIXES = {".ufw", ".fw", ".bin"}
MAX_BYTES = 1024 * 1024


def candidates() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
    ).stdout
    return [name for name in out.decode().split("\0") if name]


def problems(names: list[str]) -> list[str]:
    found = []
    for name in names:
        path = ROOT / name
        if Path(name).suffix.lower() in FIRMWARE_SUFFIXES:
            found.append(f"{name}: firmware file type")
        elif path.is_file() and path.stat().st_size > MAX_BYTES:
            found.append(f"{name}: {path.stat().st_size} bytes, more than 1 MB")
    return found


def main() -> int:
    found = problems(candidates())
    for line in found:
        print(line, file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
