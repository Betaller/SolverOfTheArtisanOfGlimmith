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
│     命中 → accept_if_valid（solver/validate.rs 验收）→ Some
│
└─ ② rose_growth::solve_rose_growth(...)   # 回退
      命中 → accept_if_valid → Some
```

`accept_if_valid`（`rose/mod.rs:72-75`）用 `solver/validate.rs` 这套**完整独立验证器**
验收——所以 rose 的解也必须过全规则校验。

### 3.1 puzzle_piece 预钉分支（`puzzle_piece + rose_window`）

`rose/mod.rs::solve_rose` 在检测到 `puzzle_piece` 规则时（门控 `ROSE_PP_PIN`，默认开）走
`solve_rose_with_pin`（`rose/mod.rs:157-`）。背景：原 `region_match.rs:285-291` 硬拒 puzzle_piece/
shape_pool 题，导致 0732 等 `puzzle_piece + rose_window` 题 aog 3s 解不出后无路可走。

**机制**（`rose/puzzle_piece_pin.rs`）：
1. `enumerate_pin_candidates`：对每个 `shape_pattern` 格，枚举 pattern 的 dihedral 变体（≤8）×
   合法放置（锚点在变体内、全在网格、不压 blocked、不跨预画边界），符号过滤为
   **恰 1 个/类型**（rose 语义：预钉即整个区域；旧「计数相等」对单类型恒真＝空转，
   0224 的组合积曾爆到 14 GB）；ring 框链 run 过滤（放置含 rim 格必须含整 run）。
2. `for_each_pin_assignment`：多锚点笛卡尔积**流式**访回（不物化全组合；MRV 锚序 +
   节点帽 2M + 指派帽 5 万 + deadline；访回返回 false 即停）。
3. 对每个 assignment：缩减 `all_positions`（移除预钉格）→ 算 `m'`（剩余每类符号数）→
   校验 `m' + n_pin == m` → 调 `region_match(m', reduced_all_positions)` → `merge_pinned`
   合并预钉区域 → `accept_if_valid`。
4. **m'=1 快速路径** `try_single_region`：剩余格若单一 4-连通分量（不跨预画边界）→ 直接成单区域，
   避开 region_match 的 `CANDIDATE_CAP=20000` 候选截断（大区域候选易被截断）。

**配套修复**：`region_match` 的种子收集（seeds / all_seed_cells）改为只从 `all_positions` 收集
（原从全盘 `puzzle.cells`），使预钉移除符号格后 `seeds.len() == m'` 自动成立。

**正确性**：shape_pattern 是 dihedral 类（`validate.rs:181-191` 比对 `dihedral_key(&region.cells)`
vs `dihedral_key(pat)`），预钉区域必须是 pattern 的某个 dihedral 变体放置——由 `accept_if_valid`
的 puzzle_piece arm 兜底校验。homogeneous 伴生题靠 validate 兜底（剩余区域碰巧同形则通过）。

**收益**（2026-08-08，分支 `rose-pp-pin`）：official puzzle_piece 159/171（基线 158，+1 = 0732 由
rose 解出，0 回归）。

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

#### `visited` 硬上限（OOM 止血）

`generate_all_candidates` 的 `visited: HashSet<CellSet>`（`region_match.rs:40`）去重集
**无界增长**——开放 rose_window 网格的 BFS 状态空间可达百万级，每条 CellSet ~88-200B，
→ OOM（exit -9）。`generate_all_candidates` 内部**无 deadline 检查**（仅 caller 有），时间
deadline 来不及防 OOM。`VISITED_CAP = 2_000_000`（`region_match.rs`）是止血阀：命中即
`break` bail-out 返回部分 `results`。

- **必须 `break`（bail out）**，不能"停插入继续 `contains` 检查"——后者去重失效致同区域
  多路径重入→指数爆炸（visited-OOM 换 queue-OOM）。
- **值选 2M**：200k 会回归 rose_window PASS 题（如 0833——真解候选在 BFS 后期被发现，
  bail 早丢弃→`match_regions_mrv` 失败→`rose_growth` 挂死）。2M × ~88-104B ≈ 176-208MB
  （低于 RSS 限制）又大到保住几乎所有可解题的完整候选集。4 道 rose OOM 中 0999 止血成功；
  0882/0826/0838 仍 OOM（根因在下游 `enum_area_combos_bounded` 无界组合枚举，非 visited）。
- **caller graceful**：部分 results → `match_regions_mrv` 可能 miss → `None` → `solve_rose`
  走 `rose_growth` fallback → `accept_if_valid`/`validate::validate` 兜底 → **仅 false-negative，
  无 false-positive**。

#### `rose_growth` deadline 修复（预存 bug）

`rose_growth.rs` 的 `solve_singlesymbol`/`solve_multisymbol` 原签名 `_deadline: Instant`
（下划线=未用）——fallback 无时间限制。当 `region_match` 返回部分候选（visited cap bail-out
后）且 `match_regions_mrv` 失败时，`rose_growth` 会**全预算挂死**（RSS 平、无输出、deadline
不触发）。修复：`solve_singlesymbol` wavefront 每 4096 步查 deadline；`solve_multisymbol`
入口 + second-pass 每 64 轮查 deadline；超时 `return None`。这让 visited cap bail-out 安全
（fallback 不再挂死，超时优雅退出）。

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
        accept_if_valid → solver/validate.rs 全套验收
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
| `area_bounds`（面积范围，共享） | `shapes.rs:115` |
| `CellSet` 位集 | `rose/cells.rs:12` |
| `edge_key` / `PreBoundaries` | `rose/cells.rs:94, 104` |
| `generate_all_candidates` BFS | `rose/region_match.rs:22` |
| `can_partition` 可达性 | `rose/region_match.rs:138` |
| `match_regions_mrv` 精确覆盖 | `rose/region_match.rs:489` |
| 单符号生长+修复 | `rose/rose_growth.rs:57` |
| 多符号生长+修复 | `rose/rose_growth.rs:289` |

---

下一节：[08-验证与约束检查](08-验证与约束检查.md)

## M1 早停的健全性边界 + `m == 2` 补集封面（2026-09-21）

`region_match::generate_all_candidates` 的 M1 早停（集齐符号类型就停止扩张）只
在「最小完整集已落在精确覆盖的可用尺寸窗口内」时健全。窗口按
`total/m/[min_sz,max_sz]` 推出（见 `docs/优化/31`）：

```
useful_min = max(min_sz, total - (m-1) * max_sz)
useful_max = min(max_sz, total - (m-1) * min_sz)
```

窗口宽 ≤ `WINDOW_GROW_LIMIT`（8）时候选 BFS 一路长到 `useful_max`，否则保留原
早停（0833 那类无尺寸约束的题不能长满）。1333（窗口 `[9,10]`）据此解出。

另加 `m == 2` 的 `try_complement_cover`：两区域平分棋盘，seed 0 的完整候选的
补集即第二区域的唯一形状，逐个做连通性 BFS 即可，最坏毫秒级。

未救回的 m==2 大盘（0974 等）是候选 BFS 在集齐异类符号前就撞 `VISITED_CAP`，
属于范式问题（`docs/优化/20` P2）。

`try_complement_cover` **必须过完整验证器**：只看补集连通性会把环顶点度 /
inequality / watchtower 全漏掉，1137 上 54ms 就 `validation_failed` 并让整条
`region_match` 路径被放弃。现在每个补集先过 `[min_sz,max_sz]` 尺寸预筛、再过
`accept_if_valid`，只返回第一个通过的；都不通过则 `None`，调用方继续正常搜索。

## `shape_pattern` 独立预钉（非 rose 前置，2026-09-21）

`puzzle_piece_pin::solve_puzzle_piece_standalone`：对「有 `puzzle_piece` 且**无**
`rose_window`」的题，在 edge_csp 之前跑一遍 shape_pattern 预钉——枚举每个锚点的
二面体放置、求两两不相交的完整指派、把剩余所有格子当成一个区域、过完整验证器。
解出 0976 / 0606 / 1215（原路由下**没有任何求解器能尝试**：aog 形状库 OOM、
rose 不适用、edge_csp 排除 `puzzle_piece`、pieces 的 DLX 没有"大无约束区域"概念
2ms exhausted、backtrack 禁用）。

两个陷阱：**deadline 必须锚到模块自己的 `Instant::now()`**（用全局 start 等于
已过期）；**给完整 unit 预算**。范围限制与 `docs/优化/32` 见该文档。

**2026-09-23 修订（前置到 aog 之前 + 三层剪枝，1215/0224 破簇）**：
- **路由**：pp-pin 独立预钉块移到 aog **之前**（门控不变：`puzzle_piece` 且
  非 rose-capable）。旧序里 aog 形状库在 1215 上 14 GB OOM 把进程带走，pp-pin
  根本轮不到；前置后 1215 **544ms via pp-pin**（旧放置树走 ~36s 仍输给 OOM）。
- **三层剪枝**（`combine_plain`）：① ring 框链 run 过滤进候选生成
  （`ring_frame_runs`：放置含 rim 格必须含整条干净 run，否则必把墙贴上框）；
  ② MRV 锚序（`pick_anchor_mrv`，含跨锚覆盖的可达性判定——一个放置可吞多锚，
  仅"全无可达放置"才是死枝）；③ 已决顶点 ring/brick 度检查（`new_pin_vertices_ok`，
  四象限全定才判，ring 禁 3、brick 禁 4）。
- **流式化**（rose 分支同享）：`enumerate_pin_assignments` 物化全组合 →
  `for_each_pin_assignment` 流式访回 + 双帽 + 恰 1 符号过滤。**0224**（12 锚
  ×单类型 rose，旧过滤恒真）从 14 GB OOM → **11ms via same-tiling**。

**2026-09-24 修订（多余数余数 `solve_multi_remainder`，0994 破簇 +1）**：
叶子原只支持「余数=单区」（0976 类）；0994 类（pattern 区 + 1 大环区 + 望塔强制
单格）余数要拆成多区。新增望塔驱动的**余数划分搜索**（`FreeRem`）：

- **望塔基数 → 成对关系**：顶点上 `value = p + k`（p = 异域 pin 标签数，自由格标签
  永不与 pin 同域），`f` 个自由格恰 `k` 个不同标签 ⇒ `f==2` 时同/异成对强制、
  `f≥3` 时 `k==f` 全异 / `k==1` 全同；预绘墙同为 must-split。must-same 经 UF 合成
  **单元**（0994 的 `(4,3)~(5,4)` 类是对角单元——其连通性只能走 `(0,3)` 走廊）。
- **搜索核**（compass_label 范式）：域过滤（可并入标签 = 潜在连通可桥接 + 非
  must-split，∪ SPAWN）+ fixpoint（joinable 潜在连通 / WT 界 / 空域判死 / 单例
  强制）+ MRV 分支 + 快照回滚。两个关键实现要点：
  1. **值域必须含全部既有标签**（不限邻接）——邻接受限 join 会漏真解：同一区的两
     个种子先后各自 spawn 后永远无法合并（0994 的 (0,0)/(1,4) 即此）；
  2. **单例强制必须逐个生效+全量重算**，不能批量——涌现标签下根节点每单元的域都
     是 `{SPAWN}`，批量强制＝每单元铸一个标签（0994 根：37 单元 → 37 标签 → WT
     界判死）。
- **v1 门控**：余数搜索需 ≥1 望塔事实才启动（0994 类的驱动约束）；无顶点线索的
  多余数类（1435 `mixed` 驱动）退化成 Bell 数划分游走，暂不接（防止 1215 类错
  叶子磨预算——单余数失败后落回下一叶子）。
- 实测：**0994 全解 ~2.8s via pp-pin**（官方预钉隔离测试 0.02s）；1215/0224/0976
  零回归。

**2026-09-24 修订2（compass+solitary 余数 `solve_compass_remainder`，1093 破簇 +1）**：
`solitary` 把自由区域数钉死为自由线索格数（1093：6 图案区 + 7 罗盘区）——自由
划分的标签**有身份**（= 罗盘格），直接委托 `compass_label::solve_labeling`
（`Model::build_excluding` 把预钉格当不可填：不进标签、不计半平面）。
两个配套修复：

1. **预绘墙 = must-differ**（compass_label 域过滤补丁，见 doc 13）：模型原把预绘墙
   当「连通断开」，但墙两侧可绕行连通 ⇒ 同标签仍合法——1093 的 `(3,1)-(4,1)`
   即被跨墙同区骗过直到 `validate` 兜底。`Model.wall_pairs` + `base_domains`
   域过滤强制异标签。
2. **每尝试 100ms 切片**：错钉叶子的无解证明可达 4.4s（1093 两个就把 30s 预算
   吃光、真叶子饿死在外）；真叶子标记解 ~10ms，100ms 是 10× 余量。
- 实测：**1093 全解 via pp-pin（+1）**；官方钉隔离 0.01s、官方放置枚举回归测试
  锁定枚举面。


## 2026-09-22 修订（预算饿死修复 + 锚点覆盖式 pp-pin + 枚举 deadline）

**rose 预算饿死（关键修复）**：路由原把 `timeout_ms - aog_elapsed` 传给 rose。
aog 的热循环 deadline 检查会超支（0382：20s 预算实际跑 45s），于是
`rose_ms = 0`、`not_attempted`——**rose+same 簇整簇因此从未被正确求解过**。改为
rose 拿完整 `timeout_ms`（与 `solve_rose` 自家锚定的语义一致，unit budget 哲学）。

**`enum_area_combos_bounded` 补 deadline**：m=30+ 种子的组合递归可达 MAX_COMBOS
上限的过程本身无界（0223 在此挂死 80s+ 直到墙杀；0826/0838/0882 同类）。加上
`Instant::now() >= deadline` 护栏后整链按时收束，0826 回流 aog 解出、0223 落到
pieces 解出。

**pp-pin 锚点覆盖式搜索（`combine_plain` 重写）**：
- 原「每锚一落点、互不相交」模型禁掉了**同形多锚共享一个区域**（0493 有 11 个锚
  挤 7 个区域，单区最多 4 锚）。改为「最低未覆盖锚 → 尝试其落点（可一次覆盖多锚）」；
- **望塔增量剪枝**（`watchtower_facts`/`watchtower_ok`）：0493 的 45 座望塔把
  25s 150 万叶子压到 33ms；
- 同类锚检查（不同形状类不可共享区域）实测会丢 0976 的干净解路径，正确性本就由
  `validate::check_puzzle_piece` 兜底，故不设（见源码注释）。

**2026-09-24 修订3（inequality 余数：尺寸窗推导，0899 破簇 +1）**：
`solve_multi_remainder` 的驱动门扩到 inequality（有向面积序 `size(a)<size(b)`，
`value==1` 翻转——对齐 `validate::edge_constraint_ok`）。核心是**尺寸窗推导**
（`collect_size_orders` + `sizes_ok`）：

- 静态界：墙一侧钉死（pin 尺寸已知）⇒ 另一侧单元的标签得 `hi = pin-1` 或
  `lo = pin+1`；两侧钉死且违序 ⇒ 预钉组合根判死；两侧自由 ⇒ 单元对序
  （同时是 must-differ——没有标签满足 `size(L)<size(L)`）。
- **可达性上界**（`label_reach` 的 extent）：标签能长到的最大格数＝其当前格
  ＋可招募桥格的可达数——被钉块围出的口袋即硬帽。错钉组合的口袋与界冲突
  在**根 fixpoint 判死**（0899 的 2.7M 预钉组合，错叶全走 µs 级根判死路径；
  正解叶子靠围袋单格强制 + 尺寸闭包直接落出）。
- 两标签都已放置时的序对窗检查：`max(lo_b, lo_a+1) > hi_b` ⇒ 死。
- 实测：**0899 全解 via pp-pin（+1）**；官方钉隔离 0.06s。

**2026-09-24 修订4（rose 基数标记 `solve_cardinal_partition`，0975a 雏形/WIP）**：
0975a 类（环纹框链 + 每类符号恰一）的专用标记搜索：标签=涌现、同型符号对
must-split、框链 run 合成单元、`rose_step` 完成度强制（缺类只剩一座桥⇒强制）
+ 标签帽=每类符号数、`spawn_completable`。**诊断结论**（供续作）：

- 官方划分直喂 `leaf_regions` **通过**（叶子/建模无误）；
- 搜索核末段不收敛：120s/1.89M 节点无一全指派（廉价域改造后 15k 节点/s），
  死亡集中在 ~70% 已派——标签走廊被错误占后的潜在连通判死；真解路径在树中
  但缺 SAC/引导序就找不到（m=2 核的 SAC-lite 尚未移植到 k 标签版）；
- 确认是**超时非穷尽**（曾误判 26k 空树穷尽——1k 节点/s 的错觉）。
- 附带产出：FreeRem `cheap_domain`（MRV/单例扫描用 O(1) 近似域、全域只算选中
  单元）——FreeRem 12× 节点提速，0994/0899/1093 零回归。
- 路由接线**暂撤**（`rose/mod.rs` 留 WIP 注释）；求解测试 `#[ignore]` 挂跟踪。

**2026-09-24 修订5（size-constraint 分区 `solve_range_partition`，1351 破簇 +1）**：
range/precise/inequality/difference 的面积约束分区（无锚）：FreeRem 尺寸窗
（`area_bounds` 静态界 + `collect_size_orders` 的钉侧/序对/差值对）+ spawn 帽
=⌊total/min⌋ + 望塔 facts 顺乘。**序/差值传递窗松弛**（`relax_size_windows`，
Bellman-Ford 式：`lo_b≥lo_a+1`、`|a−b|=v` 双向 ±v）——链式序强制具体尺寸
（0152 类纯 inequality 的关键传播）。实测：**1351 全链 4.8s via range-part
（+1）**——35 格 3 区 [17,9,9]，aog/edge_csp 双扑空。多标签高计数题
（0985 ≤16 区、0189 ≥30 区）与伴生题（0206/0928/0929 等）仍未解——
FreeRem 末段收敛是共同短板（见修订4）。求解测试 1351 锁定。

**2026-09-24 修订6（FreeRem 末段收敛三件套，0152/0270/0206 破簇 +3）**：
修订4 诊断的「末段收敛不足」共同短板，三项强化一次落地：

1. **互异链和钉死 `chain_size_deduction`**（0152 的 0.03s 破簇钥匙）：
   有向序链 `u1→…→uc`（ord 对即 must-differ，成链即两两互异尺寸）
   `s1<…<sc`、每区 ≥g ⇒ 链和 ≥ `c·g+c(c−1)/2`；链外标签每个再吃 ≥g
   ⇒ 标签数 ≤ `c+(total−floor)/g`。**和饱和情形（floor==total）直接钉死
   每环精确尺寸** `s_k=g+k−1` 且标签数=c：0152 的 7 面 inequality 墙恰好
   构成 8 元单向链，1+…+8=36=盘面 ⇒ 8 区尺寸恰为 1..8、28 个自由格只能
   按精确窗归链，搜索秒落。顺带补了 **ord 环判定**（Kahn 拓扑，Bellman-Ford
   对 `usize::MAX` 上界的环不可见）。
2. **窗口交合并检查**（`merge_statics`/`merge_window_ok`/`merge_ord_caps`）：
   join/spawn 时成员静态窗 ∩ 目标标签缓存窗 ∩ 相对已放置标签的单侧序/差
   帽（`size(La)≤sz_hi[ub]−1`、`lo≥sz_lo[oa]+1`、gap 可行性）；`sizes_ok`
   升级为 `compute_windows`（基础窗 + 单侧收紧 + **标签级 Bellman-Ford 传递
   闭包**——闭包同时抓动态序环 L1<L2<L3<L1）。0270（49 格 13 区纯
   difference）0.4s 破簇即靠窗口交把末段分支剪平。
3. **k 标签走廊强制 `corridor_force`**（compass_label §6.6 的移植）：
   某未派单元的格子是标签连通的唯一桥 ⇒ 强制并入该标签；`label_reach_vis`
   增 exclude 参数做「移除 b 还连通吗」BFS，预算 64 BFS/轮防爆。

一个修坑记录：`cap_other` 里「较大侧窗须 ≤ 对侧上界」是**误杀**（较大侧
天花板与 ohi 无关，只需收紧后窗口非空）——0899 的官方钉隔离测试立刻
抓住（正解合并被拒）。教训复用：FreeRem 家族任何新传播，先过
`*_official_pins_*` 隔离测试再谈解题数。

**2026-09-24 修订7（尺寸情形枚举 `enum_size_cases`，0929 破簇 +1）**：
区间窗看不见差值链的**析取偏移集**（`s1−s3 ∈ {±4±2} = {−6,−2,2,6}`——绝
不含 0，还带奇偶同余），修订6 的窗口交对 0929 类仍然不收敛（死因画像：
181k 条 mid-tree 尺寸窗死亡、0 条全指派）。两步定位 + 一步落地：

1. **充分性探针**（tmp_diag_0929b，隔离测试范式）：把官方各区尺寸当精确
   窗直喂搜索核 → **20 节点秒解**。证实「尺寸域钉死」是缺失钥匙。
2. **枚举代替传播**：约束图（ord∪diff）**连通**时 DFS 枚举存活尺寸元组
   （成对序/差值约束 + 和恒等式 + 标签数帽；假设约束单元各自成标签——
   合并留待搜索，漏掉的走兜底 plain），每情形一次精确窗 + 情形标签数
   spawn 帽的独立搜索。0929 仅 24 情形、真元组在列；按锚和降序（剩余升
   序）排序把真情形排到前排。**每情形 800ms 切片 + 情形总预算 6s + plain
   保底 ≥6s**（0206 类靠 plain 兜底：约 3.3s 解出）。断连约束图的笛卡尔
   积爆炸风险 → 直接跳过枚举走 plain（0270 类本就靠修订6 窗口交解出）。
3. **两个证伪**（均已回退，写此免重蹈）：
   - **k 标签 SAC-lite**（逐值 fixpoint 试死剪枝）：0 新解、且把 0206 类
     plain 末段拖慢；探针死亡 181k 但全在窗口层，剪了也没收敛。
   - **约束度引导序**（MRV 次键改 ord/diff 关联数）：0929 情形搜索受益
     但 **0206 plain 3.3s→>12s**（搜索轨迹被改坏）。正确形态是**窗口宽
     升序**（精确窗锚点优先）：情形跑里=锚点先派（0929 塌缩），plain 跑
     里宽度几乎相等=无操作（0206 无恙）。
   - 诊断开关 `RANGE_CASES=0`（跳过情形直测 plain）、死因画像计数器
     （`die_note`，cfg(test)）保留为常备诊断。
实测：**0929 全链 via range-part（+1）**；0152/0270/0206/1351/0899/0994/
1093 零回归。

**2026-10-21 修订8（number 锁三件套，0262 破簇 +1）**：
`area` 规则的格上数字 = 所在区域**精确大小**（`check_rule_area` 读
`c.number`），FreeRem 此前从未消费。当数字**互异且和=盘面**时区域数锁死
（0262：四角 14/15/18/17 和 64 = 8×8 ⇒ 4 区），此时**每个标签尺寸 ∈
已知值集 V**——三件套一次落地：

1. **`inject_number_clues`**：number 格钉其单元精确窗；互异+和恒等 ⇒
   `max_labels = |V|`，并把 `(单元, 值)` 集合作为锁交给下游。
2. **`chain_value_pins`（Hall 区间钉死，纯传播）**：严格序链
   `u1→…→uc` 必须保序嵌入有序值集 V，第 i 元只能取
   `sorted_v[i ..= i+k−c]`；`c==k` 时指派唯一（0262 两条 4 元链
   (3,5)<(3,4)<(2,4)<(2,5) 与 (5,4)<(5,3)<(5,2)<(5,1) 在
   {14,15,17,18} 上各自钉死 14<15<17<18）。2 元链则收到窄区间。
   实现要点：链=无向路径 comp 且序定向一致（唯一 indeg-0 端=最小元、
   无分叉出边）；**定向反了会把最大元钉上最小值区间**（首轮探针根判死
   即此）。含 number 单元的 comp 跳过（窗已由注入+松弛给出）。
3. **`enum_number_cases`（值指派情形枚举）**：残余约束单元（序/差值/
   must-split 端点）逐个取 V 值 DFS，成对约束即时过滤（0262 两条 2 元
   链 ⇒ 6×6=36 情形）；must-split（含预绘墙）⇒ 异值（互异 V 下=异区）。
   **排序=邻接凝聚优先**（相邻无墙单元同值=同区，真情形最大化这种合并
   ——0262 真情形得 2 分唯一最高排到 case 0），次键=锚距升序（旧的
   锚距主键把真情形排到 28/36）。number 锁下情形集**完备**（每解落在
   某情形），情形吃满模块预算不设 6s 上限、plain 仅兜底。

充分性探针（tmp_diag_0262 + `RANGE_SLICE_MS`/`RANGE_CASE_MS` 诊断旋钮）：
真情形 **527 节点 / 119ms** 秒解，28 个错情形全是 UNSAT 游走（3s 不收敛
——错钉的不可满足证明昂贵，排序是命门）。纯传播（1+2）不够：joinable
死亡 24k 纹丝不动，必须枚举钉住 2 元链的别名自由。**0262 全链 via
range-part 0.12s（+1）**；0152/0270/0206/0929/1351/0899/0994/1093
零回归。教训入库：①情形排序用「真情形会最大化什么全局合并」这类结构
信号，几何距离只配做次键；②UNSAT 情形的证明代价 ≫ SAT 情形的求解代价，
portfolio 调度靠排序不靠切片；③链定向从 indeg-0（最小元）起步。

**2026-10-21 修订9（别名感知情形 `enum_alias_gap_cases`，0839 破簇 +1）**：
修订7 的 `enum_size_cases` 假设「约束单元各自成标签」，断连约束图直接
跳过——但 0839 类（3 面差值墙的 6 个端点**别名进 3 个玫瑰种子区**）的
标签身份恰恰是谜题给的：rose 基数 m = 每类符号出现数，符号格即区域锚。

- `rose_region_count`：从符号计数取 m 与锚格（最稀有类的 m 格，一格一区）。
- `enum_alias_gap_cases`：枚举（单元 → 区域）别名（墙/序端点必异区——
  `s<s` 无解、`|a−b|=v>0` 同区无解），再对每个别名联合 DFS 区域尺寸元组
  （区域级 gap/ord 约束 + 和恒等式），每情形钉死全部约束单元与锚的精确窗。
  0839 一击 0.05s。排序复用修订8 的邻接凝聚 + 锚距次键。
- 触发条件：`enum_size_cases` 空（断连/无成员）且 rose 基数可得
  （m≤8）；number 锁路径（0262）不受影响。`max_labels` 顺带按 rose 基数
  封顶（rose 语义下区域数恒等于 m）。
- **健全性边界**：别名路径的尺寸域是「m 个区域的未知尺寸」，不是已知值
  集——`chain_value_pins`/`enum_number_cases` 的值集前提（和恒等锁）在
  无锁时**不成立**（0586 的 2/5/5/2 重复值 + 15 区不能当值集用）。
实测：**0839 全链 via range-part（+1）**；0262/0152/0270/0206/0929/1351
零回归。全语料 rose+gap/ineq 仅 0839 一道 FAIL（0840/0916/1211/0841/
0900/1208/1209/1210 本就 PASS）。

**2026-10-21 修订10（sum-lock v2 + num-pin 预通路 + 精确尺寸覆盖，基础设施批）**：
和恒等锁推广到两种形态（`check_area` 数字=区域尺寸，Σv·r_v=total 恒等）：

- **全和锁**：Σ(number cells)==total ⇒ 每 clue 独占一区（等号成立当且仅当
  r_v=c_v ∀v）——0308 的 14×5+84=154 即此，区域数=clue 数。
- **互异值和锁**：Σ(distinct values)==total ⇒ 每值一区（任一 r_v>1 即超和）
  ——0262 的 14+15+17+18、1138 的 60+61（26 个 clue 格！）即此。
  修订8 的「互异」条件实为本形态特例；`chain_value_pins`/`enum_number_cases`
  的值集前提在**任一锁下**都成立（标签尺寸 ∈ 值集），无锁（0586）仍不成立。
- **num-pin 预通路**（solver/mod.rs 链首）：锁成立才跑（非数字题零成本）。
  0262 从「aog 烧满预算 + range-part 30s」提速到 **125ms**。
- **精确尺寸覆盖搜索**（全和锁 + 小岛形状，0308 类）：最大区=残余，其余区
  连通精确尺寸候选生成（位集掩码 + 膨胀掩码）→ 动态 MRV + **MAC 弧相容
  级联**放置 → 残余连通判定。`differentiation`（相邻异区必异尺寸）在候选
  层禁止等尺寸相邻。引导走查测试（tmp_diag_0308_guided）证明剪枝不拒官方
  解；0308 本身仍未解（2.5M 节点/30s 树深不够，值序待优化——开项）。
- **FreeRem 补 differentiation 禁触传播**（相邻单元窗口精确相等 ⇒ 禁异标
  签放置）；cardinal 宿主补接 watchtower facts（此前空转）+ 单类型门放宽
  （0317 类）——均未出解（末段收敛债）。
- **attempt_module 统一链模块封装**（mod.rs）：成功即 build_solution、失败
  记 attempts 落穿，链模块样板收敛（顺带过复杂度门禁）。
诊断旋钮：`COVER_DEBUG`（候选/节点/拒绝解剖）、`RANGE_FORCE`（门禁旁路
探针）。实测：0262 via num-pin 提速 240×；口径不变（本批 0 新解，基础设施）。

**2026-10-21 修订11（region_match 健全性三则）**：
1. **匹配叶子全量校验、失败继续回溯**：结构正确的精确盖可能违反伴生规则
   （watchtower/hom-het/non_block）——原实现把第一张盖直接上交，validate
   一拒就整条路径作废。叶子处校验、拒则当死点续搜是唯一正确形态。
2. **non_block 候选层过滤 + 最小尺寸地板 3**：矩形区域永远非法（1×1/骨牌/
   直三都是矩形，仅 L 三格起），候选即弃 + 窗口地板把 0838 类单类型大 m
   的候选生长从 36 格压到 10 格。
3. **两遍候选生成（sum-forced grow-through）**：M1 早停只留最小符号跨度，
   跨度之和 < 盘面时真盖必含长大区域（1406 的 ~18 格区）——第二遍强制长穿
   （0833 类跨度之和已足则保持便宜早停）。
实测：0838/0882/0999 仍未解（候选量/组合位序的余债）；1406 solo 经
edge_csp 稳定解（预算 1.5× 后并行 6/6）；全部既有 rose 题零回归。

**2026-10-22 修订12（rose 覆盖搜索 + same/different 开门，0317 破簇 +1）**：
1. **`is_rose_capable` 解除 same/different 排除**——修订11 的叶子校验续
   回溯使该排除过时（旧行为=第一张盖上交被形状规则拒后整路径作废）。
2. **`solve_rose_cover` 三通道**：① 形状清单过滤（`different` ⇒ 尺寸 k 的
   区域数 ≤ 自由多联骨牌数 p(k)=1,1,2,5,12,35,…，0317 的多重集空间塌到
   **2 个**）；② **full-cover Algorithm X**（cell-first，`different` 异形
   互异键进搜索内放置时剪枝——0317 155ms 破簇）；③ 多重集×赋值尝试
   （最大部=残余、次大部延迟到尾部 BFS，0289 类）。
3. 完备性血泪：MAC 单例强制后扫描必须跳过已放置区（自冲突误杀）；
   尝试构建循环必须带 deadline（0325 曾跑 132s 超支）；full-cover 的
   生成上界须含最大部（残余部也是被放置区）。
4. cover 门控 m>5 且 total≤12m（0833 大均区类不进，保 region_match 管线）。
实测：**0317 via rose 0.155s、0325 via rose 2.7s（+2）**；1333/0833
管线零回归。0325 的教训：full-cover 生成上界必须取「全小部多重集」的
最大部（否则 8-10 格噪声候选把搜索树撑爆——Python 复刻 29k 节点即解的
实例，错误上界下 2M 节点无果）。0289（含 12/15 大部）走多重集赋值
通道，余债。
- **0833 回归复盘（已修）**：cover 门限按 total=54 的「真实自由格数」而非
  110 的盘面算——avg 9.0 恰好放行、cover 烧掉整个模块预算饿死
  region_match。两刀：门限收紧到 avg≤8.5（0289 8.17 保留 / 0833 9.0 出局）
  + cover 预算限 timeout/3。另证实 region_match 预算改**自锚**
  （上游全局锚会把候选生成 deadline 变成 aog 烧时的函数——0833 曾在
  第一粒种子就 deadline_past=true、候选归零）。

**2026-10-22 修订13（配对下界 + 尾部槽 + 匹配引导 slack 枚举，0289/0804 破簇 +2）**：
1. **配对下界矩阵 `pair_lb`**：rose 每区恰含每类型一格 ⇒ 区域必须同时含
   锚（P1）与其配对 P2，路径须绕开其余符号格/墙/阻塞格 ⇒ 尺寸下界
   `lb(i,j)=dist+1`。尺寸指派的**必要条件**是二分图完美匹配存在
   （`pair_match_ok`，Kuhn 增广）——0289 的全 5s 指派因 (6,5) 对下界 7
   直接判死，微秒级。残差/尾部上界用 `reach_upper`（去他锚可达分量）。
2. **尾部槽**：`assign_dfs` 允许至多一个区域取 `(defer_cap, tail_hi]`
   免候选尺寸，交给 `find_tail_set` 的余集 BFS——0289 真解
   `[15,12,7,5,5,5]` 的 12 格区恰是候选帽上外的次大区，原枚举根本
   不可达（"truth rank 2000" 是替代族幻觉，真解缺席）。
3. **残差=最大区规范化**：每个尺寸多重集都存在「最大部为余集」的分解，
   丢弃残差小于尾部/被放置区的怪异分解（廉价噪声族）不丢可达性。
   尾部上界同步收紧 `tail ≤ half`。
4. **匹配引导 slack 枚举 `enum_pair_slacks`**（0804 类）：枚举完美匹配，
   各区基座 `max(lb, min_sz)`，剩余 slack 每区 ≤2 分配——两真解皆
   slack≤2（0289 +2/+2/+0/+0/+2/+0，0804 仅一处 +2）。size DFS 的 m=9
   树在预算内只走 41-2340 叶、真解在叶序深处；本通道毫秒级出全紧致族。
5. **两相 runner**：A 相=匹配引导表（tight=max(地板数, 零过剩数) 降序，
   0804 类靠 8 个 exact-lb 区领先）；B 相=DFS 表（地板计数降序，0289
   类靠三个 size-5 地板区领先）。各半 runner 预算；每尝试切片 50ms
   （可解尝试 0.3-12ms，游荡 UNSAT 证明不得吃队列）。
6. **枚举/runner 预算分割**：枚举 ≤ 剩余 1/3 且 ≤500ms——m=9 树曾把
   6s cover 预算全烧在枚举上（runner 0 起步）。叶子成本用原始表长
   （允许边过滤只在 runner 层做：`assemble_attempt_lists` 按匹配允许
   伙伴剪候选）。CAND_CAP 20k→4k（真解最大表 2654，大噪声表生成 7×）。
7. 健全性：`pair_lb` 绕开全部其他符号格（一类型一格语义）；匹配剪枝是
   必要条件（假阴性上限=叶子走完整验证器）；规范化见 3。诊断
   `COVER_DEBUG` 增 excess/tight 直方图、相位计时、runner 停点。
实测：**0289（真解指派喂 runner 11.7ms 解）、0804（348µs 解）经
rose cover 破簇（+2）**；0317/0325/0833 全绿零回归。血泪：①尝试枚举的
截断帽会让真解"失踪"，排序再好也白搭——先证明真解在集合里（官方指派
直喂 runner 的诊断测试是标准动作）；②`max(at_min,at_lb)` 合成键曾把
0289 真解放进 1613 大带——两类真解签名不同（0289=地板岛、0804=exact-lb
包），必须分相排序而不是硬凑统一键；③枚举器每节点堆分配（匹配 scratch）
把 m=9 的 6s 预算烧光——搜索核的匹配器必须零分配。


## 数字覆盖搜索（0308 类）：口袋精确账 + 值序探索（2026-10-23）

`solve_number_cover`（全和锁、每 clue 独占一区）的精确尺寸覆盖。0308
（14 个 5 格岛 + 84 格残余）经引导走查证明剪枝健全但 2.5M 节点/30s 未收。
本轮系统性攻坚（0308 未破，机制沉淀如下）：

1. **`COVER_TRUTH=<regions.json>` 判官**：候选生成后逐 seed 核对官方解
   区域是否在其候选表——0308 全 14 个**都在**（生成完备），死因纯属值序。
   `COVER_ORDER=truth` 把真解候选顶到表首 → **15 节点秒解**，反证全部
   剪枝（含下列新增）健全。这是「排序 vs 搜索」判官的又一形态。
2. **`pocket_ok`（孤岛兜精确账）**：残余须含 `res_cell` 且连通 ⇒ 不可达
   兜必须**恰好**被兜内种子岛屿填满（|P| = Σ 兜内剩余种子尺寸），否则
   残余必断。每节点一次 BFS，把「叶子才验残余」提前到封兜即死。
3. **R 精确账**（同函数内）：`|R| == res_size + Σ R 内剩余种子尺寸` 恒等
   ——与口袋账在总和恒等式下等价，双保险。
4. **值序实验**（`COVER_ORDER=rare/mcv/wall/probe/probe-lcv`）：静态序
   （稀有格/冲突度/贴墙）无区分力（真解 wall 全 0）；动态探测序（子态
   MAC 存活质量，MCV/LCV）把树从 1.4M 压到 ~100K 节点、深度墙从 8 推到
   12，但真解在 0/2/3/8 层的 rank 为 2/13/1/4——错候选的巨型死子树
   （80 万节点级）挡道，30s 内仍够不到 14 层。
5. **迭代展宽**（`COVER_CHILD_CAP`，默认 1024）：每子树节点帽 + 截断者
   8× 升帽重试——「一个巨型死兄弟饿死浅解」的标准解法；树形显著变浅
   （depth 13 在 20M 节点处可达），但 0308 全树 >20M 节点，NODE_CAP 内
   未收。0308 开项保留：残余形状传播 / 配对下界迁移是下一步。

诊断栈沉淀：`COVER_DEBUG` 打候选表规模、深度直方图（`cover: depth-hist`）、
`COVER_TRUTH`+`COVER_DEBUG` 走真解前缀逐层打 `cover-rank: truth-rank`——
「rank 贴 0 但树仍深死」= 错候选子树肥大，指向展宽/限深而非再调启发式。


## rose cover 的 m 上限与非矩形地板（0838 类，2026-10-23/24）

`solve_rose_cover` 原有 `m > 12` 上限把 0838（non_block+rose，m=14）挡在
cover 门外，落到 region_match 单候选管线（被 non_block 拒收）。上限提到
24，并把 `try_complement_cover` 已有的 **non_block 最小尺寸地板 3**（1×1/
骨牌/I-三格皆矩形）同样装进 `try_rose_cover` 的 `min_sz`——无地板时
0838 的尺寸多重集爆到 2000 帽（min=1），有地板后塌缩为 15 个分区。

**0838 现状（开项）**：真解指派经 COVER_TRUTH 实测**在尝试集内**（5 万指派
的第 16581 位；cost 序第 9140 位）——枚举完备，纯排序/预算问题。但
2 万指派/多重集 × ~2.4ms 的逐尝试枚举范式在这个规模是 O(万) 级：
真解多重集 [3×9,4×4,6] 自身有 2 万指派，逐个试 50ms 切片不可达。
三相排序（at_min 序 / cost 序 / 反序，「分相排序」的推广）+ RUN_CAP 3k→2 万
+ cover 分额 1/3→2/3 仍差 802 个尝试。**正解是联合尺寸多重集搜索**（尺寸
不固定进 cover_dfs、带多重性追踪），逐尝试枚举到此为止——与 0308 的
「值序墙」同根：预物化候选项 + 逐项试的范式在大空间上都会撞墙。

## 大均区类 cover 搜索扩展（0975a/C4-2 类，2026-10-24）

0975a（ring+rose，m=5，真解 {61,20,15,13,12}）/ C4-2（rose，m=6）/
0844 / 1434 / 0882 等「少量大区」rose 题的 BFS 候选最大尺寸 12-13，
远小于真解均区 ~24——`span_sum < total`，combo 管线 0 area combos。
`try_rose_cover` 的门限（avg≤8.5）和 `solve_rose_cover` 的
`all_small_max` 硬退（全小部多重集为空即退）把这类题挡在 cover 门外。

**改动**：
1. **`all_small_max` 硬退改 `enum_cap` 分流**：全小类用 DEFER_CAP=10，
   大均类用 wide 多重集最大部（≤ max_sz）——`gen_cover_cands` 生成
   精确尺寸候选（DFS）而非 BFS 全尺寸。
2. **尺寸窗口 `[min_sz, total/m]`**（非 ±m 环绕均值——真解可极不均衡，
   0975a 非残余尺寸 12/13/15/20 全在 [3,24] 内）。
3. **`gen_cover_cands` 早剪 + 符号优先**：剩余格 < 缺失类型数 → 剪；
   frontier 缺失类型符号格排最后（LIFO 先探索）；`cand_cap` 大尺寸 500
   （每尝试搜索 O(cap^m)，4000^4 不可行）。
4. **`emit_attempt_leaf` 大类跳过 pairwise 预筛**（独立生成的候选在密集
   覆盖下必然重叠），排序键改「残余超额降序」（one-big 模式优先）。
5. **region_match cover 回退**：`span_sum < total` 时调 `solve_rose_cover`
   （全预算）；`m > 4` force-grow 门限移除（`collect_floor` 防小尺寸爆帽）。
6. **`enum_pair_slacks` 限 500ms**——pair 下界修正后 slack 空间膨胀，
   曾把整个 cover 预算烧光（runner 0 起步）。
7. **`non_block` 矩形过滤**（2026-10-27）：`gen_cover_cands` 对含 `non_block`
   规则的题过滤矩形候选（`is_rectangle`），防 rose 产生矩形区域被
   `accept_if_valid` 拒绝（0838 类 validation_failed 根因）。

**现状**：候选生成已到位（hostable 18-19 尺寸/种子），但 runner 每尝试
~30ms 在 500^4 组合空间内不可达；真解多重集 {12,13,15,20,61} 的排序
位置 ~35000/50000。需要联合搜索或更强剪枝。


## 断连约束图多组件情形枚举（0912 类，2026-10-23）

`enum_size_cases` 对 ord/diff 约束图**断连**的情形原本直接返回空
（「笛卡尔积爆炸，交给 plain」），0912/0928/0930d/1407a 大图断连簇的
range-part 因此只有无提示兜底 run。改为：按连通分量各自枚举可行尺寸
指派（`enum_comp_raw`，不做残余账），**笛卡尔积合并**后统一做全局残余账
（`rest = total − Σ sizes`，`rest==0 || rest≥g`）+ 标签数校验，按锚和降序
排（真解壳大先走）+ CASE_CAP 截断。case 只是窗口提示序列（末位永远有
兜底 run），错提示只损排序不损完备性——低风险。0171 类「多 clue 同区的
分组枚举」仍为开项（`number_sum_lock` 不覆盖该形态，num-pin 直接放行）。

### 2026-10-28 FreeRem 剪枝补强

- `merge_window_ok` 加弱 reach 检查（`reach >= lo/2`）：range-part 搜索中
  标签过早孤立（reach < lo）导致 `base_windows` 死亡；半阈值仅剪绝望标签，
  不影响正常搜索（完整 `reach >= lo` 会回归 0270/0804/0682）。
- `fixpoint` 加全局 lo 总和检查（`sum_lo <= total_free`）：逐标签窗口一致性
  检查遗漏跨标签超额承诺；加全局求和剪枝。
- `value_order` 相邻标签排序改为当前尺寸升序（小标签先长到 lo），
  防止标签过早孤立；修复 `compass_label::tests::solves_official_0682`。
- ~~`pick_mrv` 加 `min_adj_label_size` 排序键~~ — 已回退：与 MRV 冲突
  （优先能加入小标签的格子 = 偏好约束少的格子，与 MRV 相反），导致
  0270/0262 等 range-part 回归。`value_order` 的 `label_growth` 同理回退。
