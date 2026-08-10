#!/usr/bin/env python3
"""Stamps a project file with a different format version.

    scripts/set-format-version.py ~/Projects/CPU/base_component.slgx 9

For carrying a project to a machine running an older SimLogix. The version
is what a build checks before reading anything else, and it refuses a
document newer than itself — so a file saved here cannot be opened by the
last release until that release catches up.

Only the stamp is changed, and only when changing it is honest. Most format
versions since 6 have added optional fields and nothing else: an older build
ignores what it does not recognise, so the document still reads correctly,
just without the newer bits. Some earlier ones changed the shape of what was
already there — wires, endpoints, the container itself, the way a component
kind is written — and for those, a stamp alone produces a file that is
*misread* rather than refused, which is the worst of the three outcomes. So
this refuses to cross one and says which.

Nothing is stripped either way, so what an older build cannot read is still
in the file and comes back when you open it here again. But that build
writes back only what it understood, so **saving there drops it for good**.
That is what the backup is for. The backup is an ordinary project: rename
it to open it, or type its full name into the open dialog, since the format
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

# Whether each version added to the format without changing what was already
# there. `True` means an older build reads such a document correctly once the
# stamp is lowered past it; `False` means it does not, and this tool will not
# pretend otherwise.
#
# A version missing from here is refused rather than guessed at. `cargo test`
# holds this table to reaching CURRENT_VERSION, so bumping the format without
# saying which kind of change it was fails the build rather than being found
# out by someone's project.
ADDITIVE = {
    1: False,  # the beginning; there is nothing below to be compatible with
    2: False,  # wires became explicit, each with its own route
    3: False,  # a wire's start became a full endpoint
    4: False,  # the document became a zip container
    5: False,  # component kinds became qualified by library
    6: False,  # circuits gained folders, and a folder is part of an address
    7: True,   # components can carry properties
    8: True,   # wires can carry a colour
    9: True,   # a circuit can carry a symbol of its own
    10: True,  # a pin's name can be nudged
    11: True,  # a pin can carry an inversion bubble
    12: True,  # a component can declare how many bits its pins carry
    13: True,  # a constant carries the value it puts on its wire
    14: True,  # a splitter carries the width of each of its branches
}


def die(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


def check_reachable(current: int, target: int) -> None:
    """Refuses a stamp that would cross a version which changed the format.

    Both directions: the question is whether the versions between the two
    only ever *added*, and that does not depend on which way you are going.
    """
    low, high = min(current, target), max(current, target)
    for version in range(low, high + 1):
        if version not in ADDITIVE:
            die(
                f"format {version} is not in this tool's table — it was added "
                "without saying whether it only added to the format. Say so in "
                "ADDITIVE before using this."
            )
    # The step *into* `low` is irrelevant: nothing below it is being crossed.
    changed = [v for v in range(low + 1, high + 1) if not ADDITIVE[v]]
    if changed:
        die(
            f"format {changed[0]} changed the shape of what was already there, "
            f"so stamping across it would leave a file that is misread rather "
            f"than refused. {current} and {target} are not interchangeable."
        )


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
        # Everything before format 4 was a single JSON document, and 4 is
        # below every version this tool will stamp across anyway.
        die(f"{path} is not a project container (nothing starts with PK)")

    with zipfile.ZipFile(path) as archive:
        entries = [(info, archive.read(info.filename)) for info in archive.infolist()]

    index = next((data for info, data in entries if info.filename == "project.json"), None)
    if index is None:
        die(f"{path} has no project.json")

    current = json.loads(index).get("version")
    if not isinstance(current, int):
        die(f"{path} has no usable version (found {current!r})")
    if current == target:
        print(f"{path.name} is already version {target}; nothing written")
        return

    check_reachable(current, target)

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

