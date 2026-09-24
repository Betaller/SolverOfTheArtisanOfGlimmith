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
