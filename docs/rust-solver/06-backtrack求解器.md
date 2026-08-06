# 06 · Backtrack 求解器：最朴素的区域回溯

> 阅读对象：想理解“兜底求解器”的人。
> 前置：01（路由）、02（模型）。
> 代码：`solver/backtrack.rs`（683 行）。

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

`backtrack.rs:39-53`：

```rust
struct BacktrackState {
    cell_to_region: HashMap<(usize,usize), usize>,   // 格 → 区域号
    region_shapes:  HashMap<usize, Vec<[usize;2]>>,  // 区域号 → 格子列表
    next_region_id: usize,                           // 新区域编号
    steps: u64,                                      // 步数（超时用）
    deadline: Instant,
    area_bounds: AreaBounds,                         // 面积上下界
    watchtowers: Vec<(Vec<[usize;2]>, usize)>,       // 望塔：四格 + 目标值
}
```

### 2.1 面积上下界 `compute_area_bounds`

`backtrack.rs:55-100`：

```
初始 (1, 总数)
for 规则:
  precise → 上下界 = area
  range   → min.max(v), max.min(v)
for 每格罗盘:
  needed = 1 + up + down + left + right（至少需要这么多格）
  min_area = max(min_area, needed)
```

> 注释特别说明：`block` 和 `solitary` **不**参与面积上下界——历史上曾把 block 强设
> 成 4..4、solitary 设成 1..1，结果任何非 2×2 / 非单格的块题在结构上就不可解了。
> 这是修过的坑（`backtrack.rs:79-84`）。

### 2.2 望塔收集 `collect_watchtowers`

把每个有效望塔（1..=4）变成 `(四格列表, 目标值)`（`backtrack.rs:102-119`）。

---

## 3. 主搜索：`dfs`

`backtrack.rs:125-281`。对 `fillable` 列表（行优先）逐格处理：

```
dfs(idx):
  ① idx 到头 → check_global_constraints 叶子校验
  ② 该格已被分配 / 是 blocked → 跳到下一格
  ③ 收集四周已分配且「无分界线隔开」的相邻区域号 valid_rids
  ④ 尝试加入每个相邻区域 rid：
        - 面积上限：region 已满 max_area → 跳过
        - 预画边界冲突：若某邻居属于 rid 但被边界隔开 → 跳过
        - 面积数字：加完后 area 不能超过区内任何数字 → 否则跳过
        - check_merge_ok：不能把两个不同区域“缝合”到一起
        - 写入 → 增量检查 watchtowers + ring/brick → 递归
        - 失败 → 撤销
  ⑤ 开新区域：
        把该格作为新区域 1 号格
        增量检查 → 递归
        失败 → 撤销
```

### 3.1 关键增量检查

**`check_merge_ok`**（`backtrack.rs:284-302`）：加入 (r,c) 到 rid 后，若 (r,c) 的某邻居
属于**另一个**区域且之间无分界线，则两个区域会连通合并——不允许。

**`check_watchtowers_ok`**（`backtrack.rs:305-320`）：任何望塔顶点接触的区域数
**不能超过**目标值（部分填时取“已填格去重计数”）。

**`check_vertex_ring_ok`**（`backtrack.rs:343-373`）：新增格 (r,c) 后，检查它四角的
顶点：若 4 格已全填（或被 blocked 占掉），统计该顶点 4 条边的边界数——
`ring` 规则禁 3、`brick` 规则禁 4。

> `vertex_boundary_count`（`backtrack.rs:324-338`）统计时：blocked 相邻按“边界”计；
> 两格若未分配也按边界计（保守），所以 `check_vertex_ring_ok` 要求 4 格**全确定**
> 才触发——避免误剪。

---

## 4. 叶子校验：`check_global_constraints`

`backtrack.rs:394-646`。所有格填完后做全套最终校验：

| 检查 | 规则 |
|---|---|
| `check_watchtowers_ok` | 望塔（增量版已防超，这里再查“恰好”） |
| `check_ring_ok` | ring（全顶点扫描，`backtrack.rs:376-391`） |
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
> `build_solution` 里的 `constraints::check_all` 复查（见 01 / 03）。

---

## 5. 流程总图

```
                solve_backtrack
                     │
                     ▼
      compute_area_bounds + collect_watchtowers
                     │
                     ▼
      dfs(0)  （逐格分配区域号）
                     │
   ┌─────────────────┼──────────────────────┐
   ▼                 ▼                      ▼
 该格已分配/阻塞   加入已有相邻区域         开新区域
   │                │                      │
   │         ┌──────┴────────┐             │
   │         ▼               ▼             ▼
   │   面积上限检查      check_merge_ok
   │   预画边界冲突        （不缝合两个区）
   │   面积数字剪枝
   │         │
   │         ▼
   │   watchtowers_ok + ring/brick_ok
   │         │
   │         ▼
   │   dfs(idx+1) ──失败──► 撤销，试下一个
   │
   └─── idx 到头
              │
              ▼
      check_global_constraints（叶子全套）
              │
              ▼
        build_regions → RegionInfo
```

---

## 6. 本节代码索引

| 主题 | 位置 |
|---|---|
| `solve_backtrack` | `backtrack.rs:10` |
| `BacktrackState` | `backtrack.rs:39` |
| `compute_area_bounds` | `backtrack.rs:55` |
| `collect_watchtowers` | `backtrack.rs:102` |
| `dfs` 主循环 | `backtrack.rs:125` |
| `check_merge_ok` | `backtrack.rs:284` |
| `check_watchtowers_ok` | `backtrack.rs:305` |
| `vertex_boundary_count` | `backtrack.rs:324` |
| `check_vertex_ring_ok` | `backtrack.rs:343` |
| `check_ring_ok` | `backtrack.rs:376` |
| `check_global_constraints` | `backtrack.rs:394` |
| `build_regions` | `backtrack.rs:664` |

---

下一节：[07-rose求解器](07-rose求解器.md)
