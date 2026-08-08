from __future__ import annotations

import contextlib
import json
import os
import select
import subprocess
import threading
import time
from pathlib import Path

from src.io.puzzle_codec import puzzle_to_dict
from src.models.board import Board, Shape
from src.models.puzzle import Puzzle
from src.models.solution import RegionInfo, Solution
from src.solver.base import Solver


def _board_for(puzzle: Puzzle) -> Board:
    """Board carrying the puzzle's blocked cells and pre-drawn boundaries."""
    board = Board(puzzle.height, puzzle.width)
    for c in puzzle.cells:
        dst = board.cell(c.row, c.col)
        dst.number = c.number
        dst.symbol = c.symbol
        dst.shape_pattern = c.shape_pattern
        dst.compass = c.compass
        dst.fence_pattern = c.fence_pattern
        dst.blocked = c.blocked
    for e in puzzle.edges:
        edge = board.edge_between(e.r1, e.c1, e.r2, e.c2)
        if edge is not None:
            edge.is_boundary = e.is_boundary
    return board


def _crosses_boundary(board: Board, r0: int, c0: int, rh: int, rw: int) -> bool:
    """True if a rectangle placement overlaps any pre-drawn boundary edge."""
    for r in range(rh):
        for c in range(rw - 1):
            edge = board.edge_between(r0 + r, c0 + c, r0 + r, c0 + c + 1)
            if edge is not None and edge.is_boundary:
                return True
    for r in range(rh - 1):
        for c in range(rw):
            edge = board.edge_between(r0 + r, c0 + c, r0 + r + 1, c0 + c)
            if edge is not None and edge.is_boundary:
                return True
    return False


class _BatchLineReader:
    """Line reader over a raw pipe fd with a per-line deadline.

    Reads in chunks (not byte-by-byte) so the per-puzzle read overhead stays
    far below the subprocess-spawn cost it replaces.  A leftover partial line
    from one chunk is buffered for the next `readline`.
    """

    def __init__(self, fd: int) -> None:
        self.fd = fd
        self.buf = bytearray()

    def readline(self, deadline: float) -> str | None:
        """Return the next complete line, or `None` if `deadline` passes first
        (or EOF hits mid-line)."""
        while True:
            nl = self.buf.find(b"\n")
            if nl != -1:
                line = bytes(self.buf[:nl])
                del self.buf[: nl + 1]
                return line.decode("utf-8", errors="replace")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                return None
            readable, _, _ = select.select([self.fd], [], [], remaining)
            if not readable:
                return None
            chunk = os.read(self.fd, 8192)
            if not chunk:
                return None if not self.buf else self.buf.decode("utf-8", errors="replace")
            self.buf += chunk


def _fitting_rectangles(puzzle: Puzzle) -> list[list[list[int]]]:
    """All rectangle shapes (1x1 .. HxW) that fit at least one valid placement
    on the actual board — within fillable cells and not crossing any pre-drawn
    boundary.  A shape that fits nowhere can never appear in a solution, so it
    is dropped from the synthesized pool, shrinking it on irregular boards
    (many blocked cells / drawn boundaries).

    Used to route `block` puzzles through the shape_pool DLX solver (pieces):
    a region being a solid rectangle is exactly "region shape ∈ pool" when the
    pool is every rectangle that fits the board.
    """
    board = _board_for(puzzle)
    h, w = puzzle.height, puzzle.width
    fillable = {(c.row, c.col) for c in puzzle.cells if not c.blocked}
    shapes: list[list[list[int]]] = []
    for rh in range(1, h + 1):
        for rw in range(1, w + 1):
            fits = False
            for r0 in range(h - rh + 1):
                for c0 in range(w - rw + 1):
                    if any((r0 + r, c0 + c) not in fillable
                           for r in range(rh) for c in range(rw)):
                        continue
                    if _crosses_boundary(board, r0, c0, rh, rw):
                        continue
                    fits = True
                    break
                if fits:
                    break
            if fits:
                shapes.append([[r, c] for r in range(rh) for c in range(rw)])
    return shapes


def _find_binary() -> str:
    candidates = []

    # release build takes priority
    project_root = Path(__file__).parent.parent.parent
    for profile in ["release", "debug"]:
        candidates.append(str(project_root / "rsolver" / "target" / profile / "rsolver.exe"))
        candidates.append(str(project_root / "rsolver" / "target" / profile / "rsolver"))

    for path in candidates:
        if os.path.isfile(path):
            return path

    raise FileNotFoundError(
        "Rust solver binary not found. Build it with: cd rsolver && cargo build --release"
    )


class RustSolver(Solver):
    name = "rust"

    def __init__(self) -> None:
        self._binary = _find_binary()

    # The Rust binary runs three solver parts sequentially (aog → pieces →
    # backtrack; rose-capable puzzles swap pieces/backtrack for rose), each of
    # which gets the full unit `timeout` as its own deadline.  The subprocess
    # therefore needs 3× wall-clock for every part to use its budget.
    RUST_PARTS = 3

    # Wall-clock headroom over the 3× unit budget.  Rust's deadlines are
    # wall-clock `Instant::now()`; under `-j N` CPU contention a puzzle whose
    # CPU budget is `timeout` can take more than `timeout` of wall time, so the
    # zero-slack `3×timeout` subprocess budget would occasionally fire and (in
    # `--batch` mode) cascade the timeout to the rest of the batch.  20% slack
    # absorbs that without materially slowing the fast tail.
    SLACK = 1.2

    def _subprocess_env(self, timeout: float) -> dict[str, str]:
        """Env for the rsolver subprocess: inherit plus the per-puzzle timeout.

        `RSOLVER_TIMEOUT_MS` is the unit budget (ms) each of aog/pieces/
        backtrack/rose receives — threading `--timeout` into the Rust search
        (was hardcoded 30s in main.rs/io.rs, so `--timeout` never reached the
        solver).  Rust clamps values < 1000 to 1000.
        """
        return {**os.environ, "RSOLVER_TIMEOUT_MS": str(int(timeout * 1000))}

    def _wall_budget(self, timeout: float) -> float:
        """Subprocess wall-clock budget for one puzzle: 3× unit × slack."""
        return timeout * self.RUST_PARTS * self.SLACK

    def _prepare_input(self, puzzle: Puzzle) -> str:
        """Compact single-line puzzle JSON for the Rust subprocess."""
        data = puzzle_to_dict(puzzle)
        # block → shape_pool: hand the Rust pieces (DLX) solver a pool of every
        # rectangle up to the board size, so block puzzles use the exact-cover
        # path.  The final answer is still validated against the original `block`
        # rule by the router, so a non-rectangle tiling can never slip through.
        if puzzle.has_rule("block") and not data.get("shape_pool"):
            data["shape_pool"] = _fitting_rectangles(puzzle)
        return json.dumps(data, ensure_ascii=True)

    def _parse_solution(self, data: dict, puzzle: Puzzle) -> Solution:
        """Turn one solution-JSON dict into a Solution."""
        if not data.get("solved"):
            return Solution(
                solved=False,
                steps_taken=data.get("steps_taken", 0),
                elapsed_ms=data.get("elapsed_ms", 0),
                error_message=data.get("error_message", "No solution"),
                solver=data.get("solver", ""),
            )

        regions: list[RegionInfo] = []
        for rd in data.get("regions", []):
            cells = [(c[0], c[1]) for c in rd.get("cells", [])]
            shape_cells = [(s[0], s[1]) for s in rd.get("shape", [])]
            regions.append(RegionInfo(
                region_id=rd["region_id"],
                cells=cells,
                area=rd.get("area", len(cells)),
                shape=Shape(cells=frozenset(shape_cells)),
                normalized_shape_key=rd.get("normalized_shape_key", ""),
                matched_shape_name=rd.get("matched_shape_name"),
            ))

        board = self._board_from_regions(puzzle, regions)

        return Solution(
            solved=True,
            board=board,
            regions=regions,
            steps_taken=data.get("steps_taken", 0),
            elapsed_ms=data.get("elapsed_ms", 0),
            rule_results=data.get("rule_results", {}),
            solver=data.get("solver", ""),
        )

    def solve(self, puzzle: Puzzle, timeout: float = 30.0) -> Solution:
        input_json = self._prepare_input(puzzle)

        try:
            proc = subprocess.run(
                [self._binary],
                input=input_json,
                capture_output=True,
                text=True,
                timeout=self._wall_budget(timeout),
                encoding="utf-8",
                env=self._subprocess_env(timeout),
            )
        except subprocess.TimeoutExpired:
            return Solution(
                solved=False,
                error_message=f"Rust solver timed out after {timeout:.0f}s",
            )
        except FileNotFoundError:
            return Solution(
                solved=False,
                error_message=f"Rust solver binary not found: {self._binary}",
            )
        except Exception as e:
            return Solution(solved=False, error_message=str(e))

        if proc.returncode != 0:
            err = f"Rust solver exited with code {proc.returncode}: {proc.stderr[:500]}"
            return Solution(solved=False, error_message=err)

        try:
            data = json.loads(proc.stdout)
        except json.JSONDecodeError as e:
            return Solution(solved=False, error_message=f"Invalid JSON from solver: {e}")

        return self._parse_solution(data, puzzle)

    def solve_batch(self, puzzles: list[Puzzle], timeout: float = 30.0) -> list[Solution]:
        """Solve many puzzles in ONE rsolver `--batch` subprocess (line-delimited
        JSON in/out), reusing the process instead of spawning one per puzzle.

        Each puzzle gets its own wall-clock budget (`timeout * RUST_PARTS *
        SLACK`), read line-by-line so a puzzle that exceeds its unit `timeout`
        (threaded via `RSOLVER_TIMEOUT_MS`) is cut at its budget — exactly as
        single-puzzle mode does.  The slack headroom keeps CPU contention under
        `-j N` from cascading a single overrun into the rest of the batch.
        Returns one Solution per input puzzle, in order.
        """
        lines = [self._prepare_input(p) for p in puzzles]
        input_data = "\n".join(lines) + "\n"
        per_puzzle = self._wall_budget(timeout)
        failed = lambda msg: [  # noqa: E731
            Solution(solved=False, error_message=msg) for _ in puzzles
        ]

        try:
            # Binary mode: the per-puzzle reader does `os.read` on the raw fd,
            # so Python's buffered reader must not be in between.
            proc = subprocess.Popen(
                [self._binary, "--batch"],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                env=self._subprocess_env(timeout),
            )
        except FileNotFoundError:
            return failed(f"Rust solver binary not found: {self._binary}")
        except Exception as e:
            return failed(str(e))

        # Feed stdin in a thread so a large input can't deadlock against Rust's
        # stdout (Rust reads all input up front, then solves and writes output).
        def _feed() -> None:
            try:
                proc.stdin.write(input_data.encode("utf-8"))
                proc.stdin.flush()
            finally:
                proc.stdin.close()

        threading.Thread(target=_feed, daemon=True).start()

        results: list[Solution] = []
        reader = _BatchLineReader(proc.stdout.fileno())
        try:
            for i, puzzle in enumerate(puzzles):
                line = reader.readline(time.monotonic() + per_puzzle)
                if line is None:
                    # This puzzle exceeded its budget or the process died.
                    code = proc.poll()
                    err = (f"Rust batch died (exit {code})" if code is not None
                           else f"Rust batch timed out after {timeout:.0f}s")
                    with contextlib.suppress(Exception):
                        proc.kill()
                    while len(results) < len(puzzles):
                        results.append(Solution(solved=False, error_message=err))
                    with contextlib.suppress(Exception):
                        proc.wait()
                    return results
                try:
                    results.append(self._parse_solution(json.loads(line), puzzle))
                except json.JSONDecodeError as e:
                    results.append(Solution(
                        solved=False,
                        error_message=f"Invalid JSON from batch line {i}: {e}",
                    ))
            with contextlib.suppress(Exception):
                proc.wait()
        except Exception as e:
            with contextlib.suppress(Exception):
                proc.kill()
            while len(results) < len(puzzles):
                results.append(Solution(solved=False, error_message=str(e)))
        return results

    @staticmethod
    def _board_from_regions(puzzle: Puzzle, regions: list[RegionInfo]) -> Board:
        board = Board(puzzle.height, puzzle.width)
        for c in puzzle.cells:
            dst = board.cell(c.row, c.col)
            dst.number = c.number
            dst.symbol = c.symbol
            dst.shape_pattern = c.shape_pattern
            dst.compass = c.compass
            dst.fence_pattern = c.fence_pattern
            dst.blocked = c.blocked
        for region in regions:
            for r, c in region.cells:
                board.cell(r, c).region_id = region.region_id
        return board

    @classmethod
    def supports(cls, puzzle: Puzzle) -> bool:
        return True
