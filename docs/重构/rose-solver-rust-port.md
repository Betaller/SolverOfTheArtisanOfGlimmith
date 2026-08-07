# rose 求解器下沉到 Rust 层：设计文档

> 状态：**已实施 Phase 1-4**（cells / region_match 单符号 / rose_growth 单多符号 / 分发）。
> 关联：`docs/official-puzzles-status.md`（软门禁）、`docs/重构/data-structures.md`。
> 分工：**本窗口只做 rose 移植**；block/solitary/backtrack 建模由另一个窗口负责（`e926943` 已合入）。
> 更新（2026-08-06）：文中所引 Python 源码（`src/solver/rose/solver.py`、`region_match.py`、
> `rose_growth.py`、`bfs_candidates.py`、`candidates.py`）已随 Python 求解器栈移除
> （`docs/official-puzzles-status.md` §C.0）。本文保留作为 Rust rose 移植的设计与行为记录，
> 行为对照以 Rust 侧实现为准。

## 1. 背景与目标

当前 Rust 求解器（`rsolver/`）链 = **aog → pieces → backtrack**。aog 对**无尺寸约束的纯 rose_window 谜题**（如 `C/C4-1`、`Zone1/7-slash-pack/0277`：4×7、单符号、4 区域）在 30s 预算内解不出（已实测 `elapsed_ms: 33384`），而 Python `RoseSolver`（`src/solver/rose/solver.py` + `region_match.py` + `rose_growth.py`）约 1.4s 就解出。

**目标**：把 Python rose 求解器移植到 Rust，使 `rsolver` 二进制单独就能解这类题。这是"把 rose 及其他求解器下沉到 Rust 层、Python 求解器不再使用"的第一步。

**约束**：
- Python 求解器**先不删、但功能不使用**：`default_router()` 改为只走 `RustSolver`；Python 文件保留，待全量验证无回归后再删。
- 官方题准则：官方解唯一。
- 软门禁：每次优化更新 `docs/official-puzzles-status.md`。

## 2. 现状分析（实测 + 探索确认）

| 事实 | 依据 |
|---|---|
| C4-1 / 0277 是**单符号** rose：`len(symbol_types)==1`，M=4（每类符号出现次数），28 可填格，官方区域大小 `[14,12,1,1]` | `scan_official_results.jsonl`：0277 solver_used=`rose`，官方 sizes `[14,12,1,1]` |
| 当前 rsolver 在 C4-1 上 aog 跑满 30s 后 `{solved:false, elapsed_ms:33384}` | 实测 `rsolver ... C/C4-1.json` |
| aog 尊重 deadline（`aog/search.rs:844` 每层递归检查 `Instant::now() >= core.deadline`） | 代码确认 |
| aog 已能 <750ms 解约 30 道纯 rose 题——**分发不能回归这些** | `scan_official_results.jsonl` 纯 rose MATCH 行 elapsed 均 <750ms |
| Rust `solver/validate.rs` 是**完整独立校验器**（含真正的 `check_rose_window` `validate.rs:327-380`），可复用于 rose 解验收门 | 代码确认 |
| Rust `constraints.rs`（已删 2026-08-06）曾对 rose/fence/brick/ring/inequality/difference/watchtower/compass/puzzle_piece 是 **stub（返回 true）**，不可作验收门 | 原 `constraints.rs:35-49`（逻辑并入 `solver/validate.rs`） |
| 未解纯 rose 集：0277（Python rose 可解）、0213、0213nopad、0804、0833、1433、1434、0881g | `scan_official_results.jsonl` UNSOLVED 行 |

## 3. 模块结构

新子模块 `rsolver/src/solver/rose/`（镜像 Python 文件布局，便于逐行对照）：

```
rose/mod.rs          — 入口 solve_rose、PreBoundaries、符号类型/M 助手、build_regions
rose/cells.rs        — CellSet 位集、edge_key、PreBoundaries
rose/region_match.rs — region_match.py 的移植（单 + 多符号）
rose/candidates.rs   — generate_all_candidates（单符号）、_region_feasible_rose、_enumerate_regions（多符号）
rose/rose_growth.rs  — rose_growth.py 的移植（单 + 多符号）
```

`rsolver/src/solver/mod.rs` 加 `pub mod rose;`。

## 4. 数据结构

### CellSet（`cells.rs`）

位集，`Vec<u64>`，`idx = r*w+c`，格子上限 256 用 4 words。

```rust
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CellSet { words: Vec<u64> }
// new(n_bits), contains, insert, remove, is_disjoint, is_subset,
// union_into, len(popcount), is_empty, iter, is_full
```

选位集而非 `HashSet<(u16,u16)>`：MRV 匹配热路径 `cand & covered` / `cand.is_subset(remaining)` 是 2-4 次 u64 AND + popcount，零分配；HashSet 每次交集都哈希+分配。`derive(Hash)` 供候选 BFS 去重（Python `visited: set[frozenset]`）。

### PreBoundaries（`cells.rs`）

```rust
pub struct PreBoundaries { set: HashSet<u32> }
// from_puzzle(p)     — 从 h_edges/v_edges .is_boundary 收集
// contains(r1,c1,r2,c2)
// iter() -> [usize;4]
// endpoints() -> CellSet     // rose_growth boundary_endpoints
```

规范边键 `edge_key(r1,c1,r2,c2)` = `(r1<<24)|(c1<<16)|(r2<<8)|c2`（端点排序规范化，同 Python `_edge_key`）。

## 5. 分发（`solver/mod.rs::solve`）

```rust
fn rose_capable(p) -> bool =
    has "rose_window" 且无 size 类规则
    ("area"|"precise"|"range"|"shape_pool"|"puzzle_piece"|"compass"|"block"|"non_block"|"solitary"|"same"|"different")

const AOG_ROSE_BUDGET_MS: u64 = 5_000;   // aog 解纯 rose <750ms，5s 充裕
const ROSE_TIMEOUT_MS:    u64 = 10_000;

// rose_capable 时给 aog 短预算，aog 失败后 rose 用剩余预算：
let aog_budget = if rose_capable { AOG_ROSE_BUDGET_MS.min(timeout_ms) } else { timeout_ms };
// aog 块: deadline = start + aog_budget
// aog 失败后、AOG_ONLY 早退之前:
//   if rose_capable {
//       remaining = timeout_ms - elapsed
//       if let Some(regions) = rose::solve_rose(puzzle, &start, remaining.min(ROSE_TIMEOUT_MS)) {
//           return build_solution(regions, &start, puzzle);
//       }
//   }
```

设计取舍：**aog 先赢**（已解的 ~30 道 MATCH 不回归），rose 只在 aog 短预算失败后跑。pieces/backtrack 分支不变（纯 rose 本就到不了它们）。

## 6. 验收门

rose 返回 `Vec<RegionInfo>` 后，用 **`crate::solver::validate::validate(puzzle, &regions)`**（完整 22 规则）验收，通过才返回；分发侧走 `build_solution`（非 trusted，保留边界兜底 + rule_results）。**不要**走 `build_solution_trusted`（跳过校验）。

```rust
fn accept_if_valid(regions: Vec<RegionInfo>, puzzle: &Puzzle) -> Option<Vec<RegionInfo>> {
    if crate::solver::validate::validate(puzzle, &regions) { Some(regions) } else { None }
}
```

## 7. 算法移植顺序（各阶段可独立测试）

### Phase 1 基础（cells.rs + mod.rs 脚手架）
`CellSet` / `edge_key` / `PreBoundaries`；`rose_symbol_types`（`constraints.py:80-89`）、`rose_M`（`constraints.py:92-99`，各符号计数不等则 0）；`solve_rose` 入口返回 None。

### Phase 2 单符号 region_match（关键路径，解 C4-1/0277）
> 优化（2026-08-06）：`region_match` 感知 `range`/`precise` 的全局区域尺寸界
> （`rose::region_size_bounds`），先按 `[min,max]` 过滤候选、组合枚举
> `min_val=max(min,N)`。修复 range+rose 组合爆炸（1342 1265 万组合 → 1 个，
> 1334 → 6 个），1334/1342 由 30s FAIL → <1s 解出。

按 `region_match.py`：
1. `generate_all_candidates`（`bfs_candidates.py`）：BFS 枚举边界合规连通子集，CAP=20000、MAX_CELLS=100；单符号收集全部子集。
2. 面积预过滤：`sz <= total - (M-1)`（`region_match.py:253-264`）。
3. `_can_partition`（`region_match.py:30-97`）：剩余格从符号种子可达（不跨预画边界）+ 每个连通分量 ≥ N 格。
4. `_enum_area_combos_bounded`（`343-361`）：每区域允许大小组合（总和=total，各 ≥ max(1,N)），按方差（max-min）排序。
5. `_match_regions_mrv`（`450-533`）：递归精确覆盖——MRV 选种子（候选最少）、`cand & covered` 重叠剪枝、`sz > remaining_cells - remaining_seeds` 剪枝、`_check_boundaries_partial`（预画边两端不得同区域）、每组合 1s deadline + 全局 deadline；终点 `covered == all_positions` + 边界一致性。

### Phase 3 单符号 rose_growth（兜底）
`_solve_singlesymbol`（`rose_growth.py:35-212`）：逐区域单格种子 → wavefront 增长（选邻接已分配区域最多的未分配格，加入最小不违例区域）→ swap 修复 ≤500 次（直接移动 + 链式移动）→ `_repair_symbol_distribution` ≤200 次（把 >1 符号区域里的非符号格移到 0 符号相邻区域）。

### Phase 4 多符号
- `region_match` 多符号分支：`BacktrackSolver._generate_region_candidates`（`candidates.py:503-663`）移植 + 符号集合过滤 + 内部无预画边过滤（`region_match.py:226-245`）。
- `rose_growth` `_solve_multisymbol`（`rose_growth.py:215-357`）：多源 BFS + 二遍分配 + 修复 ≤200 次；注意 `in_same` 边界端点逻辑（`254-265`：区域不得吸收与另一预画边同区域相邻的边界端点）。

常量镜像 Python：`SWAP_REPAIR_ITER=500`、`SYMBOL_REPAIR_ITER=200`、`MULTI_REPAIR_ITER=200`、`CANDIDATE_CAP=20000`、`MAX_CANDIDATE_CELLS=100`、`PER_COMBO_TIMEOUT_MS=1000`。

### Phase 5 分发 + 测试 + 集成
接 mod.rs；`cargo test`；集成验证 C4-1/0277；纯 rose 语料回归。

## 8. Python 侧（功能不使用，暂缓）

**实际决策（2026-08-05 验证后）**：router **暂不改 Rust-only**——全量验证发现 Rust 目前解不出 3 题（1301 brick+area、1334/1342 range+rose），Python 需兜底。故：

- `default_router()` 保留 `[RustSolver(), ExactCoverSolver(), RoseSolver(), BacktrackSolver(), FallbackExactCoverSolver()]`，Rust 优先。
- Rust rose 已能解纯 rose（aog 曾超时的 C4-1/0277/0213/0213nopad）；Python rose 只在 Rust 失败后兜底。
- 待 Rust 补齐 brick+area 回溯、range+rose 求解，且 Rust-only 全量无回归后，再改 `[RustSolver()]` 并删 Python（软门禁）。

## 9. 验证

1. **cargo 单测**：CellSet 操作（含跨 word）；`edge_key` 规范化（`(1,2,1,1)==(1,1,1,2)`）；`PreBoundaries::from_puzzle`（C4-1 已知边界）；`generate_all_candidates`（2×2 带边界）；`_enum_area_combos_bounded`（`total=4,parts=2` → `[(1,3),(2,2),(3,1)]`）；`_match_regions_mrv`（2×2 手造）；`solver::validate` 对 C4-1 解为 true。
2. **集成**：`cargo build --release` 后：
   - `rsolver/target/release/rsolver puzzles/official/C/C4-1.json`
   - `rsolver/target/release/rsolver puzzles/official/Zone1/7-slash-pack/0277.json`
   - 断言 `solved:true`、`elapsed_ms<5000`、区域集合等于 Python 基准（C4-1 = `[14,12,1,1]` 布局，已实测 Python 解出）。
3. **语料回归**：`scripts/benchmark_rust_solver.py --rules rose_window`——之前 MATCH 的纯 rose 仍 MATCH（aog 或 rose），0277 等 UNSOLVED 转 solved，无新增 DIFF。
4. **全量对照**：`benchmark_rust_solver.py` 全量，对照 `docs/official-puzzles-status.md` §2（block 题回归依赖另一窗口的修复合入）。
5. 更新 `docs/official-puzzles-status.md`（软门禁）。

## 10. 风险

| 风险 | 缓解 |
|---|---|
| MRV 候选上限敏感（截断导致漏解） | 复刻 Python CAP=20000/100 格/每组合 1s，行为一致 |
| 多符号 `in_same` / 链式修复易错 | 机械移植 + `aog::validate` 验收兜底（`rose_growth.py:149-198, 254-265`） |
| MATCH→DIFF 回归（~30 道 aog 已解纯 rose） | 分发"aog 先赢短预算"，语料回归确认 |
| 大网格纯 rose（0213/0833/1433/1434/0804）仍超时 | 可接受（aog 本就失败，无回归）；后续可放宽 `ROSE_TIMEOUT_MS` |
| `build_solution` 的 constraints stub（constraints.rs 已删 2026-08-06） | 安全——`build_solution` 改用 `solver::validate` 验收，不走 trusted |

---
*最近更新：2026-08-05（方案批准，待实施）*
