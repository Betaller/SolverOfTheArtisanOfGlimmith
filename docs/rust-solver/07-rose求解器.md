# 07 · Rose 求解器：玫瑰窗专用

> 阅读对象：想理解玫瑰窗题怎么解的人。
> 前置：01（路由）、03（rose_window 规则）。
> 代码：`solver/rose/*`（约 1540 行），是 Python
> `src/solver/rose/solver.py + region_match.py + rose_growth.py` 的移植
> （Python 源码已随求解器栈移除，2026-08-06，见 `docs/official-puzzles-status.md` §C.0；
> 本文保留作为行为记录）。

---

## 1. 为什么需要单独一个求解器

`rose_window` 规则：**每个区域必须恰好包含每种符号一个**。

aog 对“无尺寸约束的纯玫瑰窗题”（如 `C/C4-1`、`Zone1/7-slash-pack/0277`：4×7、单符号、
4 区域）会在 30s 预算内挂死——因为**没有尺寸信息**，自由形状空间太大。

但玫瑰窗规则本身给了强力信息：

```
每种符号出现 m 次 → 恰好有 m 个区域！
每个区域必须含每种符号各一个 → 区域数 = m = 符号出现次数
```

Rose 求解器正是利用这点：

1. **确定区域数 m**（= 每种符号的计数）；
2. 从“最受约束的符号种子”出发生成候选区域；
3. 要么用**精确覆盖**精确匹配（region_match），要么**生长 + 修复**（rose_growth）。

---

## 2. 数据结构：`CellSet` 位集

`rose/cells.rs:12-90`。候选区域频繁做集合运算（重叠/子集判断），用 `Vec<u64>`
位集加速：

```rust
pub struct CellSet { words: Vec<u64> }   // 行优先：cell idx = r*w + c

方法：contains / insert / remove
      is_disjoint(other)   // 热门：候选与已覆盖是否重叠
      is_subset(other)
      union_into(other)    // 并集
      len()                // popcount 求和
      iter()               // 逐位迭代（trailing_zeros 技巧）
```

### 2.1 预画边界：`PreBoundaries`

`rose/cells.rs:104-147`。把所有 `is_boundary` 边编码成**规范无向边键**：

```rust
edge_key(r1,c1,r2,c2) = 端点排序后打包成 u32
// (r<<24)|(c<<16)|(r2<<8)|c2
```

`contains(r1,c1,r2,c2)` 快速判断某条邻接是否被预画边界隔断。

---

## 3. 入口：`solve_rose`

`rose/mod.rs:134-188`：

```
solve_rose(puzzle, start, timeout_ms)
│
├─ 收集 all_positions（可填格）、PreBoundaries、symbol_types、m=rose_m
├─ 若 symbol_types 空或 m==0 → None
│
├─ ① region_match::solve_by_region_match(...)
│     命中 → accept_if_valid（aog/validate.rs 验收）→ Some
│
└─ ② rose_growth::solve_rose_growth(...)   # 回退
      命中 → accept_if_valid → Some
```

`accept_if_valid`（`rose/mod.rs:125-131`）用 `aog/validate.rs` 这套**完整独立验证器**
验收——所以 rose 的解也必须过全规则校验。

---

## 4. 策略 A：region_match（精确覆盖）

`rose/region_match.rs:276-480`。思路：

```
① 选"最受约束"的符号类型（出现次数最少的）
② 对它的每个种子格，生成全部合法候选区域
③ 每个种子选一个候选，使它们互不重叠且覆盖全盘
   → 这是精确覆盖，用 MRV 回溯匹配
```

### 4.1 候选生成 `generate_all_candidates`

`region_match.rs:22-133`。BFS 生长（种子 → 可扩展候选集合）：

```
队列元素 = (当前格集, 前沿候选格集, 已含符号位掩码)

弹出一个状态：
  · 多符号：已含所有符号类型 → 记为一个候选
  · 单符号：任何连通集都记录（受 MAX_CANDIDATE_CELLS=100 限制）
  · 遍历前沿每个格：
       - 若该格是符号格且同类型符号已在当前集 → 跳过（同类型不能重复）
       - 若该格与当前集隔着预画边界 → 跳过
       - 加入后把 4 邻域新格扩进前沿 → 入队
```

> 单符号玫瑰窗里符号格可以重复吗？不能——一个区域只能含一个符号。但 `generate_all_candidates`
> 对单符号不强制“必须含符号”，只约束连通 + 边界，具体“每区恰好一符号”由后续
> 覆盖匹配和最终验收保证（区域数 m 恰好等于符号数，覆盖后自然每区一符号）。

### 4.2 预过滤

- **面积过滤**：候选面积 ∈ `[range.min, range.max]`（或 precise），
  且留足其它 `m-1` 个种子至少各 1 格。
- **可达性过滤 `can_partition`**（`region_match.rs:138-225`）：
  去掉该候选后，剩余格必须能从某个符号种子、不跨预画边界连通到；且每个连通分量
  ≥ 符号类型数（因为每区至少含每种符号一个，最小尺寸 = 类型数）。
- 按面积排序。

### 4.3 面积组合枚举

`enum_area_combos_bounded`（`region_match.rs:245-273`）枚举每个种子区域的可能面积
（m 元组，和为总面积）。按“max-min 最小”（面积最平均）优先排序，减小搜索树。

### 4.4 MRV 精确覆盖 `match_regions_mrv`

`region_match.rs:489-574`：

```
match_regions_mrv(sized, all_positions, ..., covered, assignment, ...)
│
├─ 超时 → false
├─ 找未分配种子中"兼容候选最少"的那个（MRV）
│     兼容 = 与 covered 不相交
├─ 若某种子 0 个兼容候选 → false
├─ 遍历该种子兼容候选 cand：
│     若 cand 面积 > 剩余格数 - 剩余种子数 → 跳过（防饿死）
│     写入 region_of
│     check_boundaries_partial：预画边界两端不能同区（部分检查）
│     递归
│     失败 → 撤销
└─ 全部覆盖且 covered.len()==total → true
```

`check_boundaries_partial`（`region_match.rs:230-241`）是增量边界尊重检查——确保
候选不跨预画边界放区域。

---

## 5. 策略 B：rose_growth（生长 + 修复）

`rose_growth.rs`。作为 region_match 超时/漏解的**回退**，思路更贪心：

```
① 用每种符号的第一个种子初始化 m 个区域
② 逐格把未分配格"长进"相邻区域（波前生长）
③ 修复：交换格/链式移动，消除预画边界被跨越的违例
④ 修复符号分布：让每个区域恰好含 1 个符号
```

### 5.1 单符号 `solve_singlesymbol`

`rose_growth.rs:57-287`：

- 初始化：每个种子一个区域。
- **波前生长**：反复挑“邻接区域最多”的未分配格，优先长进“当前最小”的相邻区域
  （`best_adj.sort_by_key(region_cells.len())`），并用 `would_violate` 防止跨预画边界。
- **交换修复 `SWAP_REPAIR_ITER=500` 轮**：对每条被跨越的预画边界，尝试把端点格
  换到相邻区域；不行就**链式移动**（把邻居格和自身互换区域）。
- **符号分布修复 `repair_symbol_distribution`**：把“超员符号区”的非符号格移给
  “缺符号区”。
- 最后要求每区恰好 1 个符号。

### 5.2 多符号 `solve_multisymbol`

`rose_growth.rs:289-498`：

- BFS 从种子出发，扩展时**只吃“本区未含的符号”格**；
- 边界端点格要额外检查是否会把区域“缝过”预画边界；
- 剩余未分配格：第二遍贪心填给**最小兼容区**；
- 多符号修复 `MULTI_REPAIR_ITER=200` 轮：交换格消除边界违例、维持每个区域符号位掩码
  满覆盖 `all_mask`；
- 最后 `region_symbols` 必须全等于 `all_mask` 且无未分配格。

> `region_symbols[i]` 是位掩码：bit k = 区域 i 已含第 k 种符号。
> `all_mask = (1 << 类型数) - 1` 表示“全含”。

---

## 6. 流程总图

```
                        solve_rose
                           │
      ┌────────────────────┴────────────────────┐
      │  m = rose_m（每种符号计数，不相等→None）   │
      │  symbol_types / all_positions / pre       │
      ▼                                          ▼
  region_match（首选，精确覆盖）              rose_growth（回退，贪心）
      │                                          │
  选最受约束符号类型 → 候选生成                   单符号：波前生长+交换/链式修复
  → 面积/可达性过滤 → 面积组合枚举               多符号：BFS+贪心填尾+修复
  → match_regions_mrv（MRV 精确覆盖）             → 符号分布修复
      │                                          │
      └──────────────┬───────────────────────────┘
                     ▼
        accept_if_valid → aog/validate.rs 全套验收
                     │
                     ▼
                  RegionInfo
```

---

## 7. 本节代码索引

| 主题 | 位置 |
|---|---|
| `solve_rose` 入口 | `rose/mod.rs:134` |
| `rose_m`（区域数 = 符号计数） | `rose/mod.rs:46` |
| `region_size_bounds`（面积范围） | `rose/mod.rs:99` |
| `CellSet` 位集 | `rose/cells.rs:12` |
| `edge_key` / `PreBoundaries` | `rose/cells.rs:94, 104` |
| `generate_all_candidates` BFS | `rose/region_match.rs:22` |
| `can_partition` 可达性 | `rose/region_match.rs:138` |
| `match_regions_mrv` 精确覆盖 | `rose/region_match.rs:489` |
| 单符号生长+修复 | `rose/rose_growth.rs:57` |
| 多符号生长+修复 | `rose/rose_growth.rs:289` |

---

下一节：[08-验证与约束检查](08-验证与约束检查.md)
