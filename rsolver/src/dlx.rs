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
    header_count: usize,
    pub search_count: u64,
    deadline: Option<Instant>,
    pub solution_rows: Vec<Vec<usize>>,
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
            header_count: col_count,
            search_count: 0,
            deadline: None,
            solution_rows: Vec::new(),
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
                if min_count == 0 {
                    break;
                }
            }
            c = self.nodes[c].right;
        }
        best
    }

    /// Find one solution. Returns true if found.
    pub fn search(&mut self, depth: usize) -> bool {
        if let Some(d) = self.deadline {
            if Instant::now() >= d {
                return false;
            }
        }
        self.search_count += 1;

        if self.nodes[0].right == 0 {
            return true;
        }

        let col = match self.choose_column() {
            Some(c) => c,
            None => return false,
        };

        self.cover(col);
        let c_node = col + 1;
        let mut r = self.nodes[c_node].down;
        while r != c_node {
            // Cover all columns in this row
            let mut j = self.nodes[r].right;
            while j != r {
                self.cover(self.nodes[j].col - 1);
                j = self.nodes[j].right;
            }

            if self.search(depth + 1) {
                if let Some(rid) = self.nodes[r].row_id {
                    self.solution_rows.push(vec![rid]);
                }
                return true;
            }

            // Uncover
            j = self.nodes[r].left;
            while j != r {
                self.uncover(self.nodes[j].col - 1);
                j = self.nodes[j].left;
            }
            r = self.nodes[r].down;
        }
        self.uncover(col);
        false
    }
}
