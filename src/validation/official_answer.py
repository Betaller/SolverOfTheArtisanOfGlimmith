"""Official-answer matching helper for verification / benchmark scripts.

The official corpus keeps each puzzle's canonical solution in a sibling
``*-answer`` directory, e.g.::

    puzzles/official/Zone2/10-zone2-mixed/0401.json
    puzzles/official/Zone2-answer/10-zone2-mixed/0401.json

Because the official puzzles have a unique solution, a solver that returns a
*different* (even valid) partition is a red flag.  ``matches_official_answer``
compares a solver's region partition against the canonical one, ignoring
region-id labels, so the benchmark scripts can report "solved & validated but
≠ official" as a distinct outcome.
"""
from __future__ import annotations

import json
from pathlib import Path


def _answer_path(puzzle_path: str | Path) -> Path | None:
    """Locate the sibling ``*-answer`` file for an official puzzle, if any.

    The answer dir mirrors the puzzle path with the zone component suffixed
    ``-answer`` (``Zone2/...`` → ``Zone2-answer/...``).  Non-official puzzles
    (reference / user / aiGen) have no answer dir and return ``None``.
    """
    p = Path(puzzle_path)
    parts = list(p.parts)
    for i, part in enumerate(parts):
        if part.startswith("Zone") and not part.endswith("-answer"):
            cand = Path(*parts[:i], part + "-answer", *parts[i + 1 :])
            if cand.is_file():
                return cand
    return None


def load_official_regions(puzzle_path: str | Path) -> list[list[list[int]]] | None:
    """Return the official solution's ``regions`` cell lists, or ``None`` when
    no canonical answer file exists (or it has no ``regions``)."""
    cand = _answer_path(puzzle_path)
    if cand is None:
        return None
    try:
        with open(cand, encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, json.JSONDecodeError):
        return None
    regions = data.get("regions")
    if not regions:
        return None
    return regions


def partition_key(regions) -> frozenset:
    """Normalize a region partition to a frozenset of cell-set frozensets.

    Region-id labels are ignored: two partitions are equal iff they cover the
    same cells with the same region cell-sets.
    """
    return frozenset(
        frozenset(tuple(c) for c in reg)
        for reg in regions
    )


def matches_official_answer(puzzle_path: str | Path, solution_regions) -> bool | None:
    """Whether a solver's region partition equals the official answer.

    ``solution_regions`` is a list of region cell-lists (each a list of
    ``[r, c]`` or ``(r, c)``).  Returns:
      - ``True``  — partitions match;
      - ``False`` — a canonical answer exists but the partition differs;
      - ``None``  — no official answer file is available for this puzzle.
    """
    official = load_official_regions(puzzle_path)
    if official is None:
        return None
    return partition_key(official) == partition_key(solution_regions)
