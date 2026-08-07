# 06 · Backtrack 求解器：最朴素的区域回溯

> 阅读对象：想理解“兜底求解器”的人。
> 前置：01（路由）、02（模型）。
> 代码：`solver/backtrack.rs`（811 行）。

---

## 1. 定位

`backtrack` 是路由链的**最后一道兜底**（`mod.rs:96-101`），逻辑最直白：
**按固定顺序逐格分配区域号**，不行就退回去重试。

它不需要形状目录、不需要精确覆盖——只依赖：
- 一个 `cell_to_region` 映射（格 → 区域号）；
- 一个 `region_shapes` 映射（区域号 → 格子列表）；
- 一串**增量检查**（每走一步就剪）和**叶子检查**（走完再验）。

> 因为简单，所以可靠；因为可靠，所以兜底。代价是慢，但很多题的搜索空间并不大。

---

## 2. 状态：`BacktrackState`

`backtrack.rs:56-74`：

```rust
struct BacktrackState {
    cell_to_region: Vec<Option<usize>>,              // 行优先扁平数组：格 → 区域号
    region_shapes:  Vec<Vec<[usize;2]>>,             // 区域号(=下标) → 格子列表
    next_region_id: usize,                           // 新区域编号（恒为区域列表长度）
    width: usize,                                    // 盘面宽（扁平数组 stride）
    steps: u64,                                      // 步数（超时用）
    deadline: Instant,
    area_bounds: AreaBounds,                         // 面积上下界
    watchtowers: Vec<(Vec<[usize;2]>, usize)>,       // 望塔：四格 + 目标值
    // ── area 线索专用（无 area 规则时零开销）──
    fillable: Vec<(usize,usize)>,                    // 行优先可填格列表
    cell_index: Vec<Vec<usize>>,                     // 格 → 行优先序号（blocked=MAX）
    undecided_count: usize,                          // 尚未分配的可填格数
    region_clue: HashMap<usize,usize>,               // rid → 区内心带数字的目标面积
    frontier: HashMap<usize, HashMap<(usize,usize),usize>>, // rid → {邻接未定格: 邻接数}
    has_area_rule: bool,                             // 门控：无 area 规则全跳过
}
```

> **扁平数组化（2026-08-06）**：`cell_to_region` 原为 `HashMap<(usize,usize),usize>`，
> 因格子坐标确定、`next_region_id` 严格 0..n 递增，改 `Vec<Option<usize>>`（`r*width+c`
> 索引）与 `Vec<Vec<[usize;2]>>`（区域号即下标，`push`/`pop` 维护）。`frontier` / `region_clue`
> 保持 HashMap（area 门控，转换收益低）。

### 2.1 面积上下界 `compute_area_bounds`

`backtrack.rs:78-79` 是 `shapes::area_bounds`（`shapes.rs:115`）的薄包装——面积
上下界已收敛为全 solver 共享的唯一实现（pieces / backtrack / rose 通用）：

```
初始 (1, h*w)
for 规则:
  precise → 上下界 = area
  range   → min.max(v), max.min(v)
for 每格罗盘:
  needed = 1 + up + down + left + right（至少需要这么多格）
  min_area = max(min_area, needed)
```

> `block` 和 `solitary` **不**参与面积上下界——历史上曾把 block 强设成 4..4、
> solitary 设成 1..1，结果任何非 2×2 / 非单格的块题在结构上就不可解了。这是修过的坑
> （`shapes.rs` 注释有记录）。

### 2.2 望塔收集 `collect_watchtowers`

把每个有效望塔（1..=4）变成 `(四格列表, 目标值)`（`backtrack.rs:90-107`）。

---

## 3. 主搜索：`dfs`

`backtrack.rs:113-278`。核心变化是 **`pick_next_cell` 动态选格**（替代旧版固定行优先
`fillable[idx]`）：

```
dfs(state):
  ① undecided_count == 0 → check_global_constraints 叶子校验
  ② check_area_lower_bounds（见 3.2）
  ③ (r,c) = pick_next_cell(state)     # 见 3.1
  ④ 收集四周已分配且「无分界线隔开」的相邻区域号 valid_rids
  ⑤ 尝试加入每个相邻区域 rid：
        - 面积上限：region 已满 max_area → 跳过
        - 预画边界冲突：若某邻居属于 rid 但被边界隔开 → 跳过
        - 面积数字：加完后 area 不能超过区内任何数字；且区内不能有两个
          不同目标面积的数字格（线索冲突）→ 否则跳过
        - check_merge_ok：不能把两个不同区域“缝合”到一起
        - 写入 → 维护 frontier（frontier_assign）→ 增量检查
          watchtowers + ring/brick → 递归
        - 失败 → 撤销（frontier_unassign）
  ⑥ 开新区域：把该格作为新区域 1 号格，同上 → 递归 / 撤销
```

### 3.1 动态选格 `pick_next_cell`（area 剪枝核心）

`backtrack.rs:280-312`。解决 1301 这类 **brick+area** 题的结构性死结：

> 1301（8×9，49 可填格，10 个 48 线索）用固定行优先解不出：`(0,3)` 是第 0 个可填格、
> `(1,1)` 是第 4 个，而 `(1,1)` 的所有连接格都在它之后。dfs 走到 `(1,1)` 时 48 区域还
> 长不到它旁边 → `(1,1)` 被迫开新区域 → 需要两个 48 区域（96 格 > 49 格）→ 行优先下
> **永远判 UNSAT**。官方解 = 10 个 48 线索全在同一 48 格区域 + 1 个单点。

选格规则：
- **优先长「未达目标面积」的线索区域**：遍历 `region_clue`，对每个 `area < n` 的区域，
  从其 `frontier`（邻接的未定格，引用计数）里挑行优先序号最小的格。
- 无 area 规则（`has_area_rule=false`）或没有可长的线索区域 → 退化回行优先最小未定格。

这样 48 区域从 `(0,3)` 出发按连通性 greedy 生长，`(1,1)` 先被吃掉再被处理，绕开死结。

### 3.2 面积下限剪枝 `check_area_lower_bounds`

`backtrack.rs:314-337`。两个子检查：

- **密封**：`frontier[rid]` 空 → 面积已定格，`area != n` 剪。
- **容量**：`area[rid] + undecided_count < n` 剪——剩余未定格全塞给它也不够。
  对 1301 等价于「48 区域之外最多留 1 格」，搜索从天文数字塌缩成「枚举哪 1 格做单点」。

`frontier_assign` / `frontier_unassign`（`backtrack.rs:339-412`）维护引用计数保证
undo 正确；`has_area_rule` 为 false 时全部是 no-op（无 area 规则零开销）。

### 3.3 关键增量检查

**~~`check_merge_ok`~~（已删除）**：旧逻辑「加入 (r,c) 到 rid 后，若 (r,c) 的某邻居属于
另一个区域且之间无分界线 → 拒绝」，本意是防止两个区域被缝合，但**过度保守**——相邻区域
本就合法共享边界。它把 1301 官方解的构造（单点 (6,7) 紧挨区域 0 的 (6,8)，(6,8) 加进
区域 0 时会触及区域 1）整支剪掉，导致回溯找不到官方解。删除后 1301/0957 均由 backtrack
解出；最终正确性由叶子校验 + `build_solution` 的 `validate::validate` + router 的
`IndependentValidator` 三层兜底。

**`check_watchtowers_ok`**（`backtrack.rs:414-435`）：任何望塔顶点接触的区域数
**不能超过**目标值（部分填时取“已填格去重计数”）。

**`check_vertex_ring_ok`**（`backtrack.rs:464-500`）：新增格 (r,c) 后，检查它四角的
顶点：若 4 格已全填（或被 blocked 占掉），统计该顶点 4 条边的边界数——
`ring` 规则禁 3、`brick` 规则禁 4。

> **blocked 格语义（修复，1301 关键）**：`vertex_boundary_count`（`backtrack.rs:437-462`）
> 把 blocked 格当作**空区**（`None`），**blocked-blocked 相邻不算边界**（两个空区是
> 同一 AREA_BLOCK 值），但 **blocked-区域 相邻算边界**。这是正确语义（镜像 C++
> `check_tatami` / 游戏 glimmith-solver）：
> - 旧版把 blocked 相邻**全按边界计** → 棋盘角落（3 blocked + 1 区域）假报 bc=4，
>   提前误剪解不出 1301；
> - 修复过程中曾误加「顶点有 blocked 就跳过 brick」→ **放过真 4 路交叉**：1 blocked +
>   3 个不同区域的顶点**确实是 4 路交叉**（brick 禁止），于是 1301 的孪生解
>   （单点 (7,6)，顶点 (6,5) 处 1 blocked + R0/R0/R1）被误判合法，解出与官方
>   (6,7) 不同的错解。删掉跳过、`vertex_boundary_count` 的 None/Some 规则直接给出
>   正确度数后，1301 唯一解 = 官方解。ring 检查同用该计数。

**`check_sealed_regions`**（`backtrack.rs`，2026-08-07 第一波）：无状态守卫，每次
`frontier_assign` 后从 `region_shapes`+`frontier` 重算（无 undo 逻辑）。当一个区域密封
（frontier 为空，形状已定）时，立即检查 `different`/`same`/`block`/`non_block`，
而非等到叶子。`has_different/has_same/has_block/has_non_block` 门控，无形状规则时零开销。

**`check_fence_ok`**（`backtrack.rs`，2026-08-07 专用求解器第一波 #1）：薄转发到
`solver::fence::check_fence_patterns`，把 fence 规则从事后叶子校验前置为搜索中增量剪枝。
`has_fence` 门控（无 fence 规则时单 bool 检查即返回）。详见 `solver/fence/`（独立模块，仿 `rose/`）：
- `FenceCellData` 预计算每格 `arm_count`（dihedral 不变量 = `fp.len()-1`）与 `pattern_dihedral_key`。
- 4 边界位全定时做 `dihedral_key` 比对（半成品形状的 key 无意义，必须全定）；
  未全定时用 arm-count 部分检查（`T>k` 或 `F>4-k` 即剪）。
- 消除了 8 道「校验失败」（backtrack 不再产出 fence 错解），失败模式转无解/超时。

---

## 4. 叶子校验：`check_global_constraints`

`backtrack.rs:520-764`。所有格填完后做全套最终校验：

| 检查 | 规则 |
|---|---|
| `check_watchtowers_ok` | 望塔（增量版已防超，这里再查“恰好”） |
| `check_ring_ok` | ring（全顶点扫描，`backtrack.rs:502-518`） |
| 望塔恰好计数 | 四格全确定时 distinct 数 == target |
| min_area | 每区面积 ≥ 下界 |
| 罗盘 | 每罗盘格所在区大小 ≥ 计数和 |
| area | 每数字格所在区面积 == 数字 |
| difference 边 | 水平/垂直边两侧 |面积差| == value |
| inequality 边 | 两侧面积大小关系符合（value==1 → 首端更大） |
| rose_window | 每种符号每区恰好 1 次 |
| block / non_block | 每区是否实心矩形 |
| different | 每区形状（归一化）互不相同 |
| solitary | 每区恰好 1 个线索格 |
| differentiation | 相邻区域面积不同 |

> 注意：`homogeneous` / `heterogeneous`（双子/异生边约束）**不在这里**——它依赖
> 区域形状，backtrack 的叶子检查不含形状比较；这类题的兜底实际上交给
> `build_solution` 里的 `solver/validate::validate` 复查（见 01 / 03）。

---

## 5. 流程总图

```
                solve_backtrack
                     │
                     ▼
      compute_area_bounds + collect_watchtowers + cell_index
                     │
                     ▼
      dfs(state)  （undecided_count 驱动）
                     │
                     ▼
      check_area_lower_bounds ──剪──► false
                     │
                     ▼
      pick_next_cell（优先长线索区域 frontier，否则行优先）
                     │
   ┌─────────────────┼──────────────────────┐
   ▼                 ▼                      ▼
 加入已有相邻区域   开新区域               （叶子）
   │                │                       │
   │         ┌──────┴────────┐              ▼
   │         ▼               ▼      check_global_constraints
   │   面积上限检查      check_merge_ok
   │   预画边界冲突        （不缝合两个区）
   │   面积数字/线索冲突剪枝
   │         │
   │         ▼
   │   frontier_assign + watchtowers_ok + ring/brick_ok
   │         │
   │         ▼
   │   dfs(state) ──失败──► 撤销（frontier_unassign），试下一个
   │
   └─── undecided_count == 0
              │
              ▼
        build_regions → RegionInfo
```

---

## 6. 本节代码索引

| 主题 | 位置 |
|---|---|
| `solve_backtrack` | `backtrack.rs:10` |
| `BacktrackState` | `backtrack.rs:56` |
| `compute_area_bounds`（薄包装 → `shapes::area_bounds`） | `backtrack.rs:84` |
| `collect_watchtowers` | `backtrack.rs:90` |
| `dfs` 主循环 | `backtrack.rs:113` |
| `pick_next_cell` | `backtrack.rs:280` |
| `check_area_lower_bounds` | `backtrack.rs:314` |
| `frontier_assign` / `frontier_unassign` | `backtrack.rs:339` / `374` |
| `check_watchtowers_ok` | `backtrack.rs:414` |
| `vertex_boundary_count` | `backtrack.rs:437` |
| `check_vertex_ring_ok` | `backtrack.rs:464` |
| `check_ring_ok` | `backtrack.rs:502` |
| `check_global_constraints` | `backtrack.rs:520` |
| `build_regions` | `backtrack.rs:789` |

---

下一节：[07-rose求解器](07-rose求解器.md)
