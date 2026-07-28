"""Dancing Links (DLX) — Knuth's Algorithm X for exact cover.

Ported from third_party/aog/src/dlx.rs (lifthrasiir/aog, Rust).
Uses four doubly-linked circular lists: left, right, up, down.
Column selection: MRV (minimum remaining values).

Usage:
    dlx = Dlx(num_cols)
    for rid, cols in enumerate(candidate_columns):
        dlx.add_row(rid, sorted(cols))
    solution = []
    dlx.search(solution, lambda sol: print(sol))  # finds first solution
    dlx.search_with_check(solution, row_check_fn, callback_fn)  # with pruning
"""

from __future__ import annotations


class Dlx:
    """Dancing Links — Algorithm X exact cover solver."""

    def __init__(self, num_cols: int) -> None:
        n = num_cols + 1  # root (0) + column headers (1..num_cols)
        self._num_cols = num_cols
        self.left: list[int] = list(range(-1, n - 1))
        self.left[0] = num_cols
        self.right: list[int] = list(range(1, n + 1))
        self.right[num_cols] = 0
        self.up: list[int] = list(range(n))
        self.down: list[int] = list(range(n))
        self.col: list[int] = list(range(n))
        self.row_id: list[int] = [0] * n
        self.size: list[int] = [0] * n
        self._deadline: float = float('inf')
        self._node_count: int = 0

    def add_row(self, row_id: int, cols: list[int]) -> None:
        """Add a row that covers the given column indices (must be sorted)."""
        if not cols:
            return
        base = len(self.left)
        length = len(cols)

        for i, c in enumerate(cols):
            h = c + 1  # column header node
            node = base + i

            self.left.append(node - 1 if i > 0 else base + length - 1)
            self.right.append(node + 1 if i < length - 1 else base)
            prev_up = self.up[h]
            self.up.append(prev_up)
            self.down.append(h)
            self.col.append(h)
            self.row_id.append(row_id)
            self.size.append(0)

            self.down[prev_up] = node
            self.up[h] = node
            self.size[h] += 1

    def _cover(self, c: int) -> None:
        """Remove column c from header list, and all rows in this column."""
        self.right[self.left[c]] = self.right[c]
        self.left[self.right[c]] = self.left[c]

        i = self.down[c]
        while i != c:
            j = self.right[i]
            while j != i:
                self.down[self.up[j]] = self.down[j]
                self.up[self.down[j]] = self.up[j]
                self.size[self.col[j]] -= 1
                j = self.right[j]
            i = self.down[i]

    def _uncover(self, c: int) -> None:
        """Restore column c and all rows (reverse order of _cover)."""
        i = self.up[c]
        while i != c:
            j = self.left[i]
            while j != i:
                self.size[self.col[j]] += 1
                self.down[self.up[j]] = j
                self.up[self.down[j]] = j
                j = self.left[j]
            i = self.up[i]

        self.right[self.left[c]] = c
        self.left[self.right[c]] = c

    def _choose_column(self) -> int:
        """MRV: pick column with fewest remaining rows."""
        best = self.right[0]
        best_size = self.size[best]
        c = self.right[best]
        while c != 0:
            if self.size[c] < best_size:
                best = c
                best_size = self.size[c]
                if best_size == 0:
                    break
            c = self.right[c]
        return best

    def search(self, solution: list[int], callback) -> bool:
        """Run Algorithm X. callback(sol_rows) returns True to continue search."""
        return self.search_with_check(solution, lambda _: True, callback)

    def search_with_check(self, solution: list[int], row_check, callback) -> bool:
        """Algorithm X with incremental validation.

        row_check(solution) is called after each row is added; returning False prunes.
        callback(sol_rows) is called for complete solutions; returning False stops search.
        Returns True to continue, False to stop.
        """
        if self.right[0] == 0:
            return callback(solution)

        self._node_count += 1
        if self._node_count % 50000 == 0:
            import time
            if time.monotonic() > self._deadline:
                return False

        c = self._choose_column()
        if self.size[c] == 0:
            return True

        self._cover(c)
        cont = True
        r = self.down[c]

        while r != c and cont:
            solution.append(self.row_id[r])

            j = self.right[r]
            while j != r:
                self._cover(self.col[j])
                j = self.right[j]

            if row_check(solution):
                cont = self.search_with_check(solution, row_check, callback)

            solution.pop()
            j = self.left[r]
            while j != r:
                self._uncover(self.col[j])
                j = self.left[j]

            r = self.down[r]

        self._uncover(c)
        return cont
