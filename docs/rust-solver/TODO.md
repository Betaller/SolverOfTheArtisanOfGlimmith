# rsolver 优化待办清单（TODO）

> 来源：2026-08-06 对 `rsolver/` 全量源码通读 + 文档整理时发现。
> 处理原则：每项完成时，按 `CLAUDE.md` 门禁**同步更新 `docs/rust-solver/` 对应篇**
> 并跑通 `cargo test` / `verify_puzzles.py`；改行为（优化）还需更新
> `docs/official-puzzles-status.md`。
>
> **内存专项**：本清单的 P1 #3/#4/#5、P2 #7 以及新增的「aog 形状库爆炸 / rose 候选爆炸」
> 两个 P0 级内存问题，完整调研、实测基线、根因与方案见
> **[../优化/README.md](../优化/README.md)**（内存优化系列，2026-08-06 建立）。

## 优先级总览

| 优先级 | 项目 | 收益 | 风险 | 工作量 | 状态 |
|---|---|---|---|---|---|
| P0 | 0. aog 形状库无界增长（自由形状搜索 → 5~7GB） | 极高（内存） | 中（改可解性） | 小 | 待处理，见 [../优化/README.md](../优化/README.md) |
| P0 | 0'. rose region_match 候选 `visited` 无上限 + 多层克隆（→102MB） | 高（内存） | 低 | 小 | 待处理，见 [../优化/README.md](../优化/README.md) |
| P0 | 1. 收敛重复实现 | 高（消除不一致） | 低 | 小 | **已完成**（2026-08-06，见完成记录） |
| P0 | 2. `validate.rs` 独立模块 | 高（架构清晰） | 低 | 小 | **已完成**（2026-08-06） |
| P1 | 3. backtrack 状态扁平数组 | 中 | 低 | 小 | **已完成**（2026-08-06） |
| P1 | 4. `regions_respect_boundaries` 扁平数组 | 中 | 低 | 小 | **已完成**（2026-08-06） |
| P1 | 5. `Pools`/`PlaceLevel` 惰性分配 | 中（内存） | 中 | 中 | **已完成**（2026-08-06） |
| P2 | 6. 死代码清理 | 低 | 低 | 极小 | **已完成**（2026-08-06） |
| P2 | 7. 批量模式（子进程复用） | 中（IO） | 中 | 中 | **已完成**（2026-08-06，有局限） |
| P3 | 8. `Cell` 结构体拆分 | 高 | 高 | 大 | **已完成**（2026-08-06，求解状态分离） |

> 本清单 #1~#8 **全部完成**（2026-08-06）。剩余内存专项 #0 / #0'（aog 形状库爆炸、
> rose 候选爆炸）不在此清单范围，见 `../优化/README.md`。

---

## P0 · 低风险纯重构

### 1. 收敛 5 处重复实现

同一逻辑在多处各写了一份，是不一致 bug 的温床。

> **已完成（2026-08-06）**：5 处逻辑全部收敛到 `shapes.rs`。下表「重复位置」的
> 行号是**收敛前**的（`constraints.rs` 已于 2026-08-06 删除，其逻辑并入
> `solver/validate.rs`；`aog/validate.rs` → `solver/validate.rs`）。

| 逻辑 | 重复位置（收敛前） | 唯一实现（现） |
|---|---|---|
| `dihedral_key`（8 朝向规范键） | `constraints.rs:107` 与 `aog/validate.rs:395` | `shapes.rs:32` |
| `is_rectangle` | `constraints.rs:8` 与 `aog/validate.rs:460` | `shapes.rs:12` |
| 形状池收集（顶层数组 + rule params 双来源） | `aog/core.rs:489` 与 `constraints.rs:64` | `shapes.rs:75` |
| 面积上下界计算 | `pieces.rs:125`、`backtrack.rs:55`、`rose/mod.rs:99` | `shapes.rs:115` |
| rose 符号类型收集 | `rose/mod.rs:20`、`aog/core.rs:566`、`validate.rs:329` | `shapes.rs:160` |

> 注意点：`check_mixed` 当前用 `!check_same` 实现（`constraints.rs:179`），而 `validate.rs`
> 的 mixed 是真正的「相邻异形」语义——收敛时须统一语义，别把宽松实现带过去。
> 相关文档：`03-规则与代码映射.md`、`02-数据结构.md`。

### 2. `validate.rs` 提升为独立模块

- 现状：`solver/aog/validate.rs`，但 `solver/rose/mod.rs:126` 直接
  `use crate::solver::aog::validate` —— **rose 依赖 aog**，方向反了。
- 建议：移到 `solver/validate.rs`，「完整独立验证器」不属于任何求解器。
- 相关文档：`08-验证与约束检查.md`。

---

## P1 · 性能 / 内存优化

### 3. backtrack 状态 `HashMap` → 扁平数组

- 现状：`backtrack.rs:40-41` 用 `HashMap<(usize,usize),usize>` 与
  `HashMap<usize,Vec<[usize;2]>>`。
- 依据：格子坐标确定；`next_region_id` 严格 0..n 递增（`backtrack.rs:265-278`）。
- 建议：`cell_to_region` → `Vec<Option<usize>>`（H×W 扁平）；`region_shapes` →
  `Vec<Vec<[usize;2]>>`（region_id 即下标，回溯 pop）。
- 相关文档：`06-backtrack求解器.md`。

### 4. `regions_respect_boundaries` 用扁平数组

- 现状：`mod.rs:117-158` 每次重建 `HashMap<(usize,usize),usize>` 逐格 insert。
- 建议：改 `Vec<Option<usize>>` 后直接 `rid[r*w+c]` 索引。
- 相关文档：`01-总体架构.md`、`08-验证与约束检查.md`。

### 5. `Pools`/`PlaceLevel` 惰性分配

- 现状：`aog/types.rs:224` 的 `PlaceLevel` 全是定长数组（`current_shape[256]`、
  `expand_candidates[774]`、4×`rectangle_[256]`、`stack_*[258]`×5…），单层约 20-30KB；
  `Pools::new(100)`（`aog/types.rs:297`）一次性预分配 100 层 → **常驻 2-3MB**，
  即使小棋盘也如此。
- 建议：`Pools` 按 DFS 深度**惰性扩容**（`Vec<RefCell<PlaceLevel>>` 按需 push）。
- 风险：C++ 1:1 移植的定长设计（`docs/重构/data-structures.md` 明确 aog 不在重构范围），
  改动须保持行为一致。
- 相关文档：`04-aog求解器.md`、`02-数据结构.md`。

---

## P2 · 顺手清理 / IO

### 6. 死代码清理

- `apply_line_constraint` 的 `vertical` 参数传了未用（`aog/core.rs:953`
  `let _ = vertical;`），两个调用点都传 `true`。
- `Solution.steps_taken` 恒 0（序列化到 JSON 纯浪费）；保留兼容可标注废弃。

### 7. 批量模式（子进程复用）

- 现状：Python `RustSolver` 每题 spawn 一次 rsolver 进程；`verify_puzzles.py`
  扫官方 1200+ 题时启动开销累积。
- 建议：rsolver 支持读多行 JSON、逐题求解、逐行输出，Python 侧起一个进程循环复用。
- 风险：改子进程协议 + Python 调用方，须兼容单题模式。

---

## P3 · 架构级（大改）

### 8. `Cell` 结构体拆分

- 现状：`Cell` 实测 192B（`docs/重构/data-structures.md`），主因
  `Option<String>` symbol + `Option<Shape>`；求解热路径上多数格子这些字段恒 None，
  `region_id` 甚至是死字段。
- 建议：「线索模型 vs 求解状态」分离。
- 相关文档：`02-数据结构.md`。

---

## 完成记录

| 日期 | 项目 | 结果 |
|---|---|---|
| 2026-08-06 | P0 #1（部分）`check_mixed` 语义统一 | `check_mixed` 由 `!check_same`（宽松全局近似）改为「相邻区域形状不同」正确语义（dihedral 键）；`check_same`/`check_different` 也改用 dihedral 键，修复 1114 等 `different` 误放行。见 `08-验证与约束检查.md`。**dihedral_key 等 5 处重复实现仍未收敛**，该项继续待处理。 |
| 2026-08-06 | backtrack area 剪枝（§3.1 设计落地） | `pick_next_cell` 动态连通优先生长 + `check_area_lower_bounds` 密封/容量剪枝 + frontier 引用计数；1301（brick+area）由「解不出」→ 可解。见 `06-backtrack求解器.md`。 |
| 2026-08-06 | brick 顶点 blocked 语义修复（方向修正） | `vertex_boundary_count` 把 blocked 当空区、blocked-blocked 不算边界、blocked-区域算边界，且**不跳过 blocked 顶点**——1 blocked + 3 区域 = 真 4 路交叉。同步修 `validate.rs` / `IndependentValidator` / backtrack。1301 孪生解（brick bug 假象）消除，唯一解 = 官方 (6,7)。见 `08-验证与约束检查.md`。 |
| 2026-08-06 | 删除 `check_merge_ok`（backtrack） | 「加入格若触及别的区域就拒绝」过度保守，把 1301 官方解构造整支剪掉。删除后 1301/0957 均由 Rust backtrack 解出（brick 回溯短板闭合）。正确性由叶子 + `check_all` + `IndependentValidator` 兜底。 |
| 2026-08-06 | aog 热路径 deadline（Fix B/C）+ 预算回退 | shape 循环每 256 查 deadline、size 循环每次查 deadline；去掉 1s 预算封顶（`AOG_BUDGET_CAP_MS`），aog 拿回完整 `timeout_ms` 靠 deadline 精确停住。全量回归 1047 → 983（封顶误伤 65 题）→ 回退后恢复。见 `04-aog求解器.md`。 |
| 2026-08-06 | P0 #1（补完）5 处重复实现收敛 | 新建 `shapes.rs` 集中 `dihedral_key` / `is_rectangle` / `collect_pool_shapes` / `area_bounds`（合并 pieces/backtrack/rose 三版）/ `rose_symbol_types`（合并 rose/aog/validate 三处）唯一实现；各调用方改 `crate::shapes::*`。`check_mixed` 语义保持已统一版本不动。`area_bounds` 统一默认 max=h\*w + 罗盘派生 min（rose 侧经 `min(total-(m-1))` 重界，行为不变）。见 `02/03/08`。 |
| 2026-08-06 | P0 #2 `validate.rs` 独立模块 | `solver/aog/validate.rs` → `solver/validate.rs`，消除 rose 依赖 aog 的反向依赖；`aog/mod.rs` 出口与 `rose/mod.rs` 验收改 `crate::solver::validate::validate`。见 `01/08`。 |
| 2026-08-06 | P1 #3 backtrack 状态扁平数组 | `cell_to_region` `HashMap<(r,c),rid>` → `Vec<Option<usize>>`（`r*width+c`）；`region_shapes` `HashMap<rid,Vec>` → `Vec<Vec<[usize;2]>>`（区域号=下标，push/pop）；`BacktrackState` 加 `width` stride。`frontier`/`region_clue` 保持 HashMap（area 门控）。见 `06`。 |
| 2026-08-06 | P1 #4 `regions_respect_boundaries` 扁平数组 | `mod.rs` 的 `HashMap<(usize,usize),usize>` → `Vec<Option<usize>>` 直接 `r*w+c` 索引。见 `01/08`。 |
| 2026-08-06 | P1 #5 `Pools`/`PlaceLevel` 惰性分配 | `Pools.place` → `Vec<RefCell<Option<PlaceLevel>>>`，`Pools::place_level(i)` 用 `RefMut::map` + `get_or_insert_with` 按 DFS 深度惰性建层。峰值 RSS：A1-1 5.6→2.3MB、C1-3 5.7→2.9MB、C4-1 11.7→9.0MB（此前 100 层 × ~33KB 常驻 ~3.3MB）。见 `04`、`results/20260806_pools-lazy-rss.txt`。 |
| 2026-08-06 | P2 #6 死代码清理 | 移除 `apply_line_constraint` 的 `vertical` 参数、`grid::unassigned_cells`/`connected_components`、`polyomino::generate_polyominoes`、aog `dbg_steps`/`slash_check_*`/重复 `has_shape_pool`、`Dlx::search`/`solution_rows`/`header_count`、`CellSet::set_from`/`PreBoundaries::len`、`types::Direction`/`CompassClue::get`、未用参数（`pick_next_cell` puzzle、`check_edge_constraints` regions）；`Solution.steps_taken` 保留兼容并标注废弃；`main.rs` 文档字符串修正。 |
| 2026-08-06 | P2 #7 批量模式（子进程复用） | rsolver `--batch`（多行 JSON 逐行进出）+ IO 移出 main.rs（新 `io.rs`）+ `RustSolver.solve_batch`（每题独立预算，`select`+`os.read` 分块读，超时只截断该题与后续）+ `verify_puzzles.py`/`benchmark_rust_solver.py` `--batch N`。**局限**：某题超出内部 30s 预算（大 rose runaway）会连带同批后续题超时，精确验证用 `--batch 1`；快题吞吐 ~6×。见 `01`、README。 |
| 2026-08-06 | P3 #8 `Cell` 结构体拆分（求解状态分离） | 删除 `Cell.region_id`（求解路径死字段，16B）与 `assigned()`（唯一使用者 `grid::unassigned_cells` 已随 #6 移除）；求解归属状态落在各求解器自有结构。Cell ~192B → ~176B。Python `board.py` 的 `Cell.region_id` 是独立模型，不受影响。见 `02`、`docs/重构/data-structures.md`。 |
| 2026-08-06 | 全量 verify 围栏/罗盘失败根因修复（新增，非清单项） | 全量 verify 暴露 30 题「答案未通过独立验证」（全为 fence/compass/ring/rose 相关）。根因：`constraints.rs` 9 条规则为恒 `true` 的 stub。**删除 `constraints.rs`**，`build_solution`/pieces 改用 `solver/validate::validate` 全量复查。30 题改为 Rust 内诚实拒绝（仍 FAIL 但不再上报错误解）。0 回归，36/36 抽样解与官方解一致。`verify_puzzles.py`/`benchmark_rust_solver.py` 新增官方解比对（`matches_official`，DIFF 即失败）。见 `08`、`docs/official-puzzles-status.md` 第一部分/第二部分、`results/20260806_82c9132_verify-full.txt`。 |
| 2026-08-06 | 边界望塔缺失修复（新增，非清单项） | 用户报告 0800/0543 官方题边界有望塔、JSON 缺失。根因：转换器（`convert_archive.py` 只收集内部行/列）+ 模型（顶点数组内部 `(h-1)×(w-1)`，`build_puzzle` 拒绝边界坐标）**双双丢弃边界望塔**。修复：顶点约定改**绝对网格坐标**（`0..=h × 0..=w`），转换器收集全部边界望塔，85 个 watchtower 谜题 JSON 以 `puzzles.json` 迁移。**watchtower DIFF 全部消除（0 DIFF）**，6 道解出官方解、0985 改诚实超时。0 回归（35 个 watchtower FAIL 全为基线既有）。见 `02`、`08`、`docs/official-puzzles-status.md` 附录 A / 第一部分 / 第二部分、`src/ui/grid_widget.py`。 |

（每完成一项在此登记，并更新顶部总览的状态列。）
