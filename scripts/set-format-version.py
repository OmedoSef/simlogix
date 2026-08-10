#!/usr/bin/env python3
"""Stamps a project file with a different format version.

    scripts/set-format-version.py ~/Projects/CPU/base_component.slgx 9

For carrying a project to a machine running an older SimLogix. The version
is what a build checks before reading anything else, and it refuses a
document newer than itself — so a file saved here cannot be opened there
until the release catches up.

Only the stamp is changed. Nothing is stripped, so anything the older build
does not understand is still in the file and comes back when you open it
here again. But that build ignores what it cannot read, and **writes back
only what it understood** — so saving there drops it for good. That is what
the backup is for.

The backup is written beside the file and is an ordinary project: to open
it, rename it, or type its full name into the open dialog, since the format
is recognised from the bytes rather than the extension.

Entries are rewritten uncompressed, which is how the application stores
them — a deflate stream would be rewritten wholesale by a one-character
edit, and that is what makes a container hostile to version control.
"""

import json
import shutil
import sys
import zipfile
from pathlib import Path


def die(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


def main() -> None:
    if len(sys.argv) != 3:
        die("usage: set-format-version.py <project.slgx> <version>")

    path = Path(sys.argv[1])
    try:
        target = int(sys.argv[2])
    except ValueError:
        die(f"{sys.argv[2]!r} is not a version number")

    if not path.is_file():
        die(f"{path} is not a file")
    if path.read_bytes()[:2] != b"PK":
        # Everything before format 4 was a single JSON document. Those are
        # all far below any version worth stamping, so refusing says more
        # than handling them would.
        die(f"{path} is not a project container (nothing starts with PK)")

    with zipfile.ZipFile(path) as archive:
        entries = [(info, archive.read(info.filename)) for info in archive.infolist()]

    index = next((data for info, data in entries if info.filename == "project.json"), None)
    if index is None:
        die(f"{path} has no project.json")

    current = json.loads(index).get("version")
    if current == target:
        print(f"{path.name} is already version {target}; nothing written")
        return

    backup = path.with_suffix(f".v{current}{path.suffix}.bak")
    shutil.copy2(path, backup)

    written = path.with_suffix(path.suffix + ".new")
    with zipfile.ZipFile(written, "w") as archive:
        for info, data in entries:
            if info.filename == "project.json":
                document = json.loads(data)
                document["version"] = target
                data = json.dumps(document, indent=2).encode()
            fresh = zipfile.ZipInfo(info.filename, date_time=info.date_time)
            fresh.compress_type = zipfile.ZIP_STORED
            fresh.external_attr = info.external_attr
            archive.writestr(fresh, data)
    shutil.move(written, path)

    # Read it back rather than trusting the write, and hold every other
    # entry to being untouched: this runs on the only copy of someone's work.
    with zipfile.ZipFile(backup) as before, zipfile.ZipFile(path) as after:
        if before.namelist() != after.namelist():
            die("the entries came out in a different order — restore from the backup")
        changed = [n for n in before.namelist() if before.read(n) != after.read(n)]
        if changed != ["project.json"]:
            die(f"unexpectedly rewrote {changed} — restore from the backup")
        read_back = json.loads(after.read("project.json"))["version"]
        if read_back != target:
            die(f"reads back as {read_back} — restore from the backup")

    print(f"{path.name}: version {current} → {target}")
    print(f"backup: {backup}")
    if target < current:
        print(
            "note: an older build drops what it cannot read when it saves. "
            "Keep the backup until you are back on this version."
        )


if __name__ == "__main__":
    main()
