# rsolver 优化待办清单（TODO）

> 来源：2026-08-06 对 `rsolver/` 全量源码通读 + 文档整理时发现。
> 处理原则：每项完成时，按 `CLAUDE.md` 门禁**同步更新 `docs/rust-solver/` 对应篇**
> 并跑通 `cargo test` / `verify_puzzles.py`；改行为（优化）还需更新
> `docs/official-puzzles-status.md`。

## 优先级总览

| 优先级 | 项目 | 收益 | 风险 | 工作量 | 状态 |
|---|---|---|---|---|---|
| P0 | 1. 收敛重复实现 | 高（消除不一致） | 低 | 小 | 部分（mixed 语义已统一，5 处重复未收敛） |
| P0 | 2. `validate.rs` 独立模块 | 高（架构清晰） | 低 | 小 | 待处理 |
| P1 | 3. backtrack 状态扁平数组 | 中 | 低 | 小 | 待处理 |
| P1 | 4. `regions_respect_boundaries` 扁平数组 | 中 | 低 | 小 | 待处理 |
| P1 | 5. `Pools`/`PlaceLevel` 惰性分配 | 中（内存） | 中 | 中 | 待处理 |
| P2 | 6. 死代码清理 | 低 | 低 | 极小 | 待处理 |
| P2 | 7. 批量模式（子进程复用） | 中（IO） | 中 | 中 | 待处理 |
| P3 | 8. `Cell` 结构体拆分 | 高 | 高 | 大 | 待处理 |

---

## P0 · 低风险纯重构

### 1. 收敛 5 处重复实现

同一逻辑在多处各写了一份，是不一致 bug 的温床。

| 逻辑 | 重复位置 | 建议落点 |
|---|---|---|
| `dihedral_key`（8 朝向规范键） | `constraints.rs:107` 与 `aog/validate.rs:395` | 收敛到 `types.rs` 或新 `shapes.rs` |
| `is_rectangle` | `constraints.rs:8` 与 `aog/validate.rs:460` | 同上 |
| 形状池收集（顶层数组 + rule params 双来源） | `aog/core.rs:489` 与 `constraints.rs:64` | 同上 |
| 面积上下界计算 | `pieces.rs:125`、`backtrack.rs:55`、`rose/mod.rs:99` | 同上 |
| rose 符号类型收集 | `rose/mod.rs:20`、`aog/core.rs:566`、`validate.rs:329` | 同上 |

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

（每完成一项在此登记，并更新顶部总览的状态列。）
