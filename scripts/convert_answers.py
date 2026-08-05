"""Convert the archive's official solutions into per-puzzle answer files.

The archive (third_party/archiveofglimmith.github.io/puzzles.json) stores every
official puzzle's region-boundary answer in its ``solution`` field.  This script
decodes that ASCII grid into a region partition (see
``convert_archive.archive_solution_regions``) and writes one answer file per
puzzle, mirroring the puzzle layout:

    puzzles/official/Zone1-answer/1-single-shape/0008.json

Answer file format:

    {
      "version": "1.0",
      "grid": {"height": 6, "width": 5},
      "regions": [[[0, 0], [0, 1], ...], ...],   # one list of [row, col] per region
      "_meta": {"archive_id": "0008", "archive_type": "1-single-shape", ...}
    }

Puzzles whose archive entry carries no official solution (0067, 1130) are
skipped, matching the puzzle files that already exist for them.

Usage:
    python scripts/convert_answers.py            # write all answer files
    python scripts/convert_answers.py --dry-run  # only report, don't write
"""
from __future__ import annotations

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from convert_archive import ARCHIVE_PATH, OUT_ROOT, archive_solution_regions

ZONES = ("Zone1", "Zone2", "Zone3")


def convert_all(dry_run: bool = False) -> tuple[int, int, list[str]]:
    """Write answer files under puzzles/official/{zone}-answer/.  Returns
    (written, skipped_no_solution, errors)."""
    with open(ARCHIVE_PATH, encoding="utf-8") as f:
        data = json.load(f)

    written = 0
    skipped = 0
    errors: list[str] = []
    for p in data:
        zone = p.get("zone")
        if zone not in ZONES:
            continue
        ptype = p.get("type", "misc")
        pid = p.get("id", "unknown")
        try:
            regions = archive_solution_regions(p)
        except Exception as e:  # noqa: BLE001
            errors.append(f"{zone}/{ptype}/{pid}: {e}")
            continue
        if not regions:
            skipped += 1
            continue

        out = {
            "version": "1.0",
            "grid": {"height": int(p["height"]), "width": int(p["width"])},
            "regions": [[list(cell) for cell in region] for region in regions],
            "_meta": {
                "archive_id": pid,
                "archive_type": ptype,
                "archive_difficulty": p.get("difficulty"),
            },
        }

        if dry_run:
            written += 1
            continue
        out_dir = os.path.join(OUT_ROOT, f"{zone}-answer", ptype)
        os.makedirs(out_dir, exist_ok=True)
        with open(os.path.join(out_dir, f"{pid}.json"), "w", encoding="utf-8") as f:
            json.dump(out, f, ensure_ascii=False, indent=2)
        written += 1

    return written, skipped, errors


def main() -> None:
    dry_run = "--dry-run" in sys.argv
    written, skipped, errors = convert_all(dry_run=dry_run)
    print(f"{'Dry-run' if dry_run else 'Converted'} {written} answer files "
          f"({skipped} without official solution)")
    if errors:
        print(f"Errors ({len(errors)}):")
        for e in errors:
            print("  " + e)
        sys.exit(1)


if __name__ == "__main__":
    main()
