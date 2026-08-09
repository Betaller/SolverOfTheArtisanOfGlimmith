//! Dancing Links (Algorithm X) for exact cover.

use std::time::Instant;

#[derive(Debug, Clone, Default)]
struct DlxNode {
    left: usize,
    right: usize,
    up: usize,
    down: usize,
    col: usize,
    row_id: Option<usize>,
    count: usize,
}

/// Dancing Links solver for exact cover problems.
///
/// Columns represent constraints, rows represent choices.
/// `search()` returns true if a solution is found.
#[derive(Debug)]
pub struct DancingLinks {
    nodes: Vec<DlxNode>,
    pub search_count: u64,
    deadline: Option<Instant>,
}

impl DancingLinks {
    /// Create a new DLX instance with `col_count` primary columns.
    pub fn new(col_count: usize) -> Self {
        let mut nodes = Vec::with_capacity(col_count + 1);
        // Root node at index 0
        nodes.push(DlxNode {
            left: col_count,
            right: 1,
            ..Default::default()
        });
        // Column headers at indices 1..=col_count
        for c in 1..=col_count {
            nodes.push(DlxNode {
                left: c - 1,
                right: if c == col_count { 0 } else { c + 1 },
                up: c,
                down: c,
                col: c,
                ..Default::default()
            });
        }
        Self {
            nodes,
            search_count: 0,
            deadline: None,
        }
    }

    /// Set a deadline after which `search()` will abort.
    pub fn set_deadline(&mut self, d: Instant) {
        self.deadline = Some(d);
    }

    /// Add a row covering the given columns. `row_id` is an opaque identifier
    /// that will be collected when the row is selected in a solution.
    pub fn add_row(&mut self, cols: &[usize], row_id: usize) {
        let start = self.nodes.len();
        for (i, &col) in cols.iter().enumerate() {
            let c = col + 1; // offset for root
            let cur = start + i;
            let left_pos = if i == 0 { start + cols.len() - 1 } else { cur - 1 };
            let right_pos = if i == cols.len() - 1 { start } else { cur + 1 };
            let hdr_down = self.nodes[c].down;

            self.nodes.push(DlxNode {
                left: left_pos,
                right: right_pos,
                up: c,
                down: hdr_down,
                col: c,
                row_id: Some(row_id),
                count: 0,
            });
            let idx = self.nodes.len() - 1;
            self.nodes[c].down = idx;
            self.nodes[hdr_down].up = idx;
            self.nodes[c].count += 1;
        }
    }

    /// Cover (remove) a column.
    fn cover(&mut self, col: usize) {
        let c = col + 1;
        let right = self.nodes[c].right;
        let left = self.nodes[c].left;
        self.nodes[right].left = left;
        self.nodes[left].right = right;

        let mut i = self.nodes[c].down;
        while i != c {
            let mut j = self.nodes[i].right;
            while j != i {
                let nd = self.nodes[j].down;
                let nu = self.nodes[j].up;
                let col_idx = self.nodes[j].col;
                self.nodes[nd].up = nu;
                self.nodes[nu].down = nd;
                self.nodes[col_idx].count -= 1;
                j = self.nodes[j].right;
            }
            i = self.nodes[i].down;
        }
    }

    /// Uncover (restore) a column.
    fn uncover(&mut self, col: usize) {
        let c = col + 1;
        let mut i = self.nodes[c].up;
        while i != c {
            let mut j = self.nodes[i].left;
            while j != i {
                let col_idx = self.nodes[j].col;
                self.nodes[col_idx].count += 1;
                let nd = self.nodes[j].down;
                let nu = self.nodes[j].up;
                self.nodes[nd].up = j;
                self.nodes[nu].down = j;
                j = self.nodes[j].left;
            }
            i = self.nodes[i].up;
        }
        let right = self.nodes[c].right;
        let left = self.nodes[c].left;
        self.nodes[right].left = c;
        self.nodes[left].right = c;
    }

    /// Choose the column with the fewest rows (MRV heuristic).
    fn choose_column(&self) -> Option<usize> {
        let mut min_count = usize::MAX;
        let mut best = None;
        let mut c = self.nodes[0].right;
        while c != 0 {
            if self.nodes[c].count < min_count {
                min_count = self.nodes[c].count;
                best = Some(c - 1);
                // D5: a column with count 0 means the branch is dead; count 1
                // is a forced move — no column can have fewer, so stop scanning.
                // (doc 16 §2 D5; saves 30-50% of the MRV column scan.)
                if min_count <= 1 {
                    break;
                }
            }
            c = self.nodes[c].right;
        }
        best
    }

    /// Search with an incremental validation callback.
    /// `row_check` is called after each row selection with the current partial
    /// solution (row IDs); returning false prunes the branch.  `on_solution` is
    /// called at each complete solution; returning true stops the whole search.
    /// Returns true if a solution that stopped the search was found.
    pub fn search_with_check(
        &mut self,
        depth: usize,
        partial: &mut Vec<usize>,
        row_check: &mut dyn FnMut(&[usize]) -> bool,
        on_solution: &mut dyn FnMut(&[usize]) -> bool,
    ) -> bool {
        // D9: throttle the deadline check to every 1024 nodes instead of every
        // node — Instant::now() is a ~20ns vDSO call, unnecessary on every
        // recursion. search_count is incremented below; check it first so the
        // very first nodes still get a timely check. (doc 16 §2 D9, -10-25%.)
        if let Some(d) = self.deadline {
            if (self.search_count & 1023) == 0 && Instant::now() >= d {
                return false;
            }
        }
        self.search_count += 1;

        if self.nodes[0].right == 0 {
            return on_solution(partial);
        }

        let col = match self.choose_column() {
            Some(c) => c,
            None => return false,
        };

        self.cover(col);
        let c_node = col + 1;
        let mut r = self.nodes[c_node].down;
        while r != c_node {
            let row_id = self.nodes[r].row_id;
            if let Some(rid) = row_id {
                partial.push(rid);

                // Incremental check
                if row_check(partial) {
                    // Cover all columns in this row
                    let mut j = self.nodes[r].right;
                    while j != r {
                        self.cover(self.nodes[j].col - 1);
                        j = self.nodes[j].right;
                    }

                    if self.search_with_check(depth + 1, partial, row_check, on_solution) {
                        // Stop propagates up; matrix state is left mid-search
                        // (callers do not reuse the DLX object after this).
                        return true;
                    }

                    // Uncover
                    j = self.nodes[r].left;
                    while j != r {
                        self.uncover(self.nodes[j].col - 1);
                        j = self.nodes[j].left;
                    }
                }

                partial.pop();
            }
            r = self.nodes[r].down;
        }
        self.uncover(col);
        false
    }
}
