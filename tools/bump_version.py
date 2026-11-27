#!/usr/bin/env python3
"""Raises OpenPeriph's version: python3 tools/bump_version.py patch|minor|major|none

Changes Cargo.toml and Cargo.lock, and turns "## Unreleased" in CHANGELOG.md into
"## vX.Y.Z (date)". Used by the Release workflow; works the same locally."""

import datetime
import pathlib
import re
import sys

root = pathlib.Path(__file__).resolve().parent.parent
part = (sys.argv[1] if len(sys.argv) > 1 else "patch").split()[0]

cargo = root / "Cargo.toml"
text = cargo.read_text()
old = re.search(r'^version = "(\d+)\.(\d+)\.(\d+)"', text, re.M)
major, minor, patch = map(int, old.groups())
if part == "major":
    major, minor, patch = major + 1, 0, 0
elif part == "minor":
    minor, patch = minor + 1, 0
elif part == "patch":
    patch += 1
elif part != "none":
    sys.exit(f"unknown part {part!r}: use patch, minor, major or none")
new = f"{major}.{minor}.{patch}"
cargo.write_text(text[: old.start()] + f'version = "{new}"' + text[old.end():])

lock = root / "Cargo.lock"
lock.write_text(re.sub(r'(name = "openperiph"\nversion = )"[^"]*"', rf'\g<1>"{new}"', lock.read_text()))

log = root / "CHANGELOG.md"
if log.exists():
    today = datetime.date.today().isoformat()
    t = log.read_text()
    if f"## v{new} " not in t and f"## v{new}\n" not in t:
        t = t.replace("## Unreleased", f"## Unreleased\n\n## v{new} ({today})", 1)
    log.write_text(t)

print(f"version {'.'.join(old.groups())} -> {new}")
