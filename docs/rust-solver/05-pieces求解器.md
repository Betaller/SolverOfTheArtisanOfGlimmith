# 05 · Pieces 求解器：DLX 精确覆盖

> 阅读对象：想理解“把放形状变成精确覆盖”的人。
> 前置：01（路由）、02（模型）。
> 代码：`solver/pieces.rs`（666 行）+ 通用引擎 `dlx.rs`（270 行）。

---

## 1. 核心思想：精确覆盖问题

一个形状池 / 面积数字 / 罗盘题，可以这样抽象：

```
谜题 = 一堆积木（候选区域），要求：
  · 每块积木是一组格子（连通、面积合规、满足线索）
  · 选出一批积木，它们两两不重叠
  · 并且覆盖所有可填格

这就是经典「精确覆盖」(Exact Cover) 问题：
  列 = 每个格子
  行 = 每个候选区域（覆盖若干列）
  解 = 选若干行，使每列恰好被覆盖一次
```

Dancing Links（DLX）是解精确覆盖的经典算法（Knuth 的 Algorithm X 的双向链表实现），
用 **MRV 启发式**（先选候选最少的列）快速搜索。

---

## 2. 精确覆盖的直觉例子

一个 2×2 盘面，两个候选“横条”：

```
列（格子）：   c0  c1  c2  c3        ← 每个格子一列
           ┌────────────────┐
行 A（横条[0,1]）│  1   1   0   0 │
行 B（横条[2,3]）│  0   0   1   1 │
           └────────────────┘
选 A+B → 每列恰好 1 个 1 → 精确覆盖 → 解
```

DLX 搜索时：选一列 → 选覆盖它的行 → 删掉该行覆盖的所有列 → 递归。

---

## 3. 主流程：`solve_pieces`

`pieces.rs:33-79`：

```
solve_pieces(puzzle, _start, timeout_ms)
│
├─ 若无任何线索 且 无形状池 → None（交回路由走 backtrack）
├─ build_context(puzzle)              # 格子编号、面积上下界、望塔/边约束表
├─ generate_all_placements(...)       # ① 生成全部候选区域
│     └─ 空 → None
├─ 建 DLX：列 = 格子数，每候选一行
│     DancingLinks::new(num_cells) + dlx.add_row(cell_ids, i)
├─ dlx.search_with_check(0, &mut partial, row_check, on_solution)
│     · row_check 恒 true（DLX 本身已保证不重叠/全覆盖）
│     · on_solution 对每个完整覆盖调用 reconstruct_and_validate：
│          验证通过 → 记录答案并返回 true（停止搜索）
│          验证失败 → 返回 false（继续找下一个覆盖）
└─ result
```

> 关键点：DLX 找到的“第一个完整覆盖”不一定满足全局规则（比如 block 题的第一个
> 覆盖往往是全 1×1 单格）。所以 `on_solution` 会**一直搜到合法覆盖或超时**。

---

## 4. 候选生成：`generate_all_placements`

`pieces.rs:218-311`。三路来源：

### 4.1 形状池候选

```
for 每个池形状:
  for 每个 8 朝向变换 transform:
    for 每个锚点 (r,c):
      try_place(transform, r, c) → 不越界且不压 blocked 格 → 记录候选
```

一个 `Placement`：

```rust
struct Placement {
    cells: Vec<[usize;2]>,    // 具体格子
    area: usize,
    shape: Shape,             // 归一化形状
    cell_ids_flat: Vec<usize>,// DLX 覆盖的列号
}
```

### 4.2 面积数字候选

对每个带 `number` 的格 `(sr,sc)`、面积 `size`：

```
generate_polyominoes(puzzle, sr, sc, size)
```

用 `poly_rec`（`pieces.rs:380-428`）递归生成**所有含该格、连通、恰好 size 格**的
多连块。注意候选集合 `candidates` 用 `BTreeSet`，并跳过 blocked 格与预切边
（`is_precut`）。

> 面积数字会限定区域大小，所以这里生成的候选**天然满足 area 规则**。
>
> **面积目标上限 `MAX_AREA_TARGET = 12`**（`pieces.rs:33-36`）：面积目标 > 12 时
> 跳过 DLX 候选生成、留给 backtrack——枚举大面积的连通多连块会爆炸（如 1301 的
> 面积 48）。阈值沿用已移除的 Python `ExactCoverSolver`（`max(targets) <= 12`，
> 2026-08-06 随 Python 求解器栈删除，见 `docs/official-puzzles-status.md` §C.0）。

### 4.3 罗盘候选

只对“强约束罗盘”生成（≥3 方向指定，或左右/上下成条带型），
`generate_compass_polyominoes`（`pieces.rs:431-572`）：

```
compass_rec 递归生长：
  · 每步更新 counts[4]（北/南/东/西计数）
  · 任何方向超过罗盘指定值 → 剪枝
  · 四个方向都恰好满足 → 记录候选（并停止该分支）
  · 候选总数上限 MAX_COMPASS_PLACEMENTS = 2000（防爆）
```

### 4.4 去重

所有候选按覆盖格子的有序集合去重（`pieces.rs:302-309`）。

---

## 5. DLX 引擎（`dlx.rs`）

### 5.1 数据结构：双向十字链表

```rust
struct DlxNode {
    left, right, up, down: usize,   // 四向链表指针（数组下标）
    col: usize,                     // 所属列头
    row_id: Option<usize>,          // 该行对应的候选编号
    count: usize,                   // 列头专用：该列行数
}
```

用 `Vec<DlxNode>` 池化分配。布局：

```
节点 0 = 根（横链表头）
节点 1..=col_count = 列头
其后 = 数据行节点（每行一个环，串在列链表上）

     根 ──► 列头1 ──► 列头2 ──► ... ──► 根
            │          │
            ▼          ▼
          行节点环    行节点环
```

### 5.2 操作

- `add_row(cols, row_id)`：把一行挂到各列链表尾部（`dlx.rs:66-89`）。
- `cover(col)`：从横链表摘下列头，并把该列所有行的**其它列**从列链表里摘除
  （`dlx.rs:92-113`）。
- `uncover(col)`：逆向恢复（`dlx.rs:116-136`）。
- `choose_column()`：选**行数最少**的列（MRV，`dlx.rs:139-154`）。

### 5.3 `search` / `search_with_check`

```
search(depth):
  超时 → false
  根右为空（无未覆盖列）→ true（找到解）
  选 MRV 列 → cover
  for 该列的每行 r:
    cover 该行覆盖的其它列
    search(depth+1) → 命中则把 r 记入 solution_rows
    uncover 恢复
  uncover 列 → false

search_with_check(depth, partial, row_check, on_solution):
  多两个回调：
    · 选行后先 row_check(partial)（本工程恒 true）
    · 解完整时 on_solution(partial)——返回 true 才停止整个搜索
```

---

## 6. 回跳验证：`reconstruct_and_validate`

`pieces.rs:584-666`。DLX 给出一个完整覆盖（一堆行号）后：

```
reconstruct_and_validate(puzzle, placements, row_ids, ctx)
│
├─ 覆盖检查：所有可填格都被某候选覆盖
├─ 边约束：遍历 edge_constraints
│     inequality → 两侧区域面积不同
│     difference → |面积差| == value
│     delta      → 两侧形状不同
│     gemini     → 两侧形状相同
├─ 望塔：每望塔顶点接触的不同区域数 == target
├─ 组装 RegionInfo
└─ constraints::check_all(puzzle, rules, regions)  # 块/相异/差异化/独居/形状池……
      全部规则通过才返回 Some(regions)
```

> 也就是说：DLX 负责“几何上不重叠、全覆盖”，而**规则合法性由 `check_all` 兜底**。
> 这样一条规则也不漏。

---

## 7. 路由怎么走进 pieces

`solve()`（`mod.rs:85-93`）：

```
走 pieces ⟺ has_shape_pool || has_area_clues || has_compass_clues
```

- 有形状池 → pieces 直接放池形状。
- 有面积数字 → pieces 生成面积候选（比回溯穷举快得多）。
- 有强罗盘 → pieces 生成罗盘候选。
- 都不满足 → backtrack。

---

## 8. 流程总图

```
                    solve_pieces
                        │
                        ▼
                build_context
        （格子编号 / 面积上下界 / 望塔 / 边约束表）
                        │
                        ▼
         generate_all_placements ──► Vec<Placement>
        （形状池 8朝向 平铺 + 面积数字多连块 + 强罗盘）
                        │
                        ▼
              DLX 矩阵（列=格子，行=候选）
                        │
                        ▼
       search_with_check（MRV 选列，回溯精确覆盖）
                        │
              ┌─────────┴──────────┐
              ▼                    ▼
       找到完整覆盖           超时/无覆盖
              │                    │
              ▼                    ▼
   reconstruct_and_validate   返回 None（回退到 backtrack）
        │
        ├─ 边约束/望塔/规则全过 → Some(regions)
        └─ 有一项不过 → 继续搜下一个覆盖
```

---

## 9. 本节代码索引

| 主题 | 位置 |
|---|---|
| `solve_pieces` 主流程 | `pieces.rs:33` |
| `build_context` | `pieces.rs:93` |
| 面积上下界（共享 `shapes::area_bounds`） | `shapes.rs:115` |
| 候选生成 `generate_all_placements` | `pieces.rs:218` |
| 形状池候选 | `pieces.rs:224-245` |
| 面积数字候选 `generate_polyominoes` / `poly_rec` | `pieces.rs:247-270, 359-428` |
| 罗盘候选 `generate_compass_polyominoes` | `pieces.rs:272-300, 431-572` |
| `is_precut` 预切边判断 | `pieces.rs:574` |
| `reconstruct_and_validate` | `pieces.rs:584` |
| DLX 数据结构 | `dlx.rs:5-27` |
| `search_with_check` | `dlx.rs:209` |

---

下一节：[06-backtrack求解器](06-backtrack求解器.md)
