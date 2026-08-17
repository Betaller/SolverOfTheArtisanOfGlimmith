# aog/backtrack 挂死根因与 deadline 盲区（第四轮调研）

> 状态：**根因分析**文档。3 个 agent 深度调研"LB: sealed"非确定性挂死，**颠覆了团队记忆的两个误判**。
> 调研日期：2026-08-09。基线：1052/1258 PASS（206 FAIL）。
> 关联：`optimization-progress-tracker`（待办 P0）、`compass-bbox-falsified-rose-visited-landed`（挂死首次发现）。

---

## 0. 核心结论

**"LB: sealed" 挂死的真正根因是 backtrack 的 HashMap 非确定性，不是 aog 死循环。**

1. "LB: sealed" 唯一打印点在 `backtrack.rs:696`（`check_area_lower_bounds`），**不在 aog**。
   团队记忆"aog 的 empty_area_check/BF 死循环"是定位错误。
2. **没有任何函数有真死循环**：BF 有收敛检测+硬上界；empty_area_check 有界 O(cells²)；
   backtrack dfs 是标准回溯。生产路径 deadline 正常在 40s 触发。
3. 非确定性来自 `pick_next_cell`（backtrack.rs:637-662）遍历 `HashMap`（Rust RandomState 种子随机）
   → 选格顺序非确定 → 搜索树形状每次不同 → 0749 有时 1884ms PASS、有时 40000ms timeout。
4. "deadline 不触发"是 `AOG_DEBUG=1` 时百万行 `eprintln!` 阻塞 I/O 的假象，生产路径不触发。
5. **真正的 aog 隐患**是 empty_area_check 无 deadline + 4096 粒度太粗（单次重步数秒），
   虽非死循环但超时响应慢——这与 backtrack 非确定性是两个独立问题。

---

## 1. "LB: sealed" 定位（纠正误判）

### 1.1 唯一打印点

`rsolver/src/solver/backtrack.rs:696`，函数 `check_area_lower_bounds`（687-709）：

```rust
if crate::aog_debug_enabled() && is_sealed(rid) && area != n {
    eprintln!("LB: sealed rid={} area={} n={}", rid, area, n);
}
```

调用链：`solve_backtrack` → `dfs`(backtrack.rs:299) → 行311 `check_area_lower_bounds` → 行696 打印。

**grep 全仓确认 "LB: sealed" 只此一处**。aog 的 `empty_area_check`(empty.rs:328)、
`dfs_empty_area`(empty.rs:186)、`propagate_area_bounds`(prototypes.rs:15) **均无此字符串**。

### 1.2 不是死循环

`check_area_lower_bounds` 本身不是循环——找到首个 sealed-and-area-wrong 的 region 就 `return false`。
`dfs` 是标准递归回溯：行305 `steps+=1`，行306 每1024步查 deadline，行311 剪枝，行349 选格，分支递归。

**没有 while-true 无 break、没有不收敛不动点**。"无限打印"真相：
- dfs 被进入数百万次（搜索树大），每次调 `check_area_lower_bounds` 打一行 → 日志洪水
- `eprintln!` 同步阻塞 stderr，百万行把进程变 I/O-bound，`steps` 增长极慢，40s 内到不了 1024
  → deadline 检查没机会触发 → 表象"deadline 不触发"
- RSS 平（内存不涨）因纯搜索循环，不分配形状库（backtrack 状态已是扁平数组）

**无 AOG_DEBUG 时**（benchmark）：不打印，steps 正常推进，deadline 在 40000ms 正常触发。
bench 里 0749/0829/0875 显示 `FAIL ... 40000ms timeout`。

### 1.3 BF 不会死循环（证伪记忆假设）

`propagate_area_bounds`(prototypes.rs:15-87)：
- 外层 `for _pass in 0..num_regions`（行47）——硬上界
- 每轮 `if !changed { break; }`（行78）——不动点收敛退出
- lb/ub 单调收紧，最多 num_regions 轮必收敛

记忆"BF 不收敛"假设不成立。

---

## 2. 非确定性根因：backtrack 的 HashMap

### 2.1 根因

`pick_next_cell`(backtrack.rs:637-662) 遍历两个 HashMap：
- `state.region_clue: HashMap<usize, usize>`（行643）
- `state.frontier.get(&rid): HashMap<(usize,usize), usize>`（行647）

Rust `HashMap` 用 `RandomState`，种子每次进程启动随机化 → 迭代顺序非确定 → 选格非确定 →
搜索树形状每次不同 → 0749 有时 1884ms PASS、有时 40000ms timeout。

**铁证**：同题同二进制，`latest.jsonl` vs `231d8d2 bench`，0749 一会 PASS(2266ms via aog)
一会 FAIL(40000ms)；0829 PASS(28163ms)/FAIL(40000ms)；0875 PASS(26359ms)/FAIL(40000ms)。

### 2.2 aog 侧也有非确定性源

`shape_digest_index: HashMap<u32, Vec<usize>>`、`node_to_shape_index: HashMap<(i32,i32), Vec<usize>>`
(core.rs:27-28)、`place_visited: HashMap<u64,i32>`(types.rs:219)——决定形状库命中顺序。
但那是 aog OOM 题(8x10m22-watch 类)的根因，与 0749/0829/0875 的 backtrack 挂死无关。

### 2.3 题规则确认（纠正记忆归类）

| 题 | 规则 | 能打印 LB:sealed? |
|---|---|---|
| 0749 | area + rose | 是（有 area-number） |
| 0829 | block + area + solitary | 是 |
| 0875 | differentiation + area + solitary | 是 |
| 1333 | range + rose | **否**（无 area-number，记忆归类有误） |
| 0833 | rose | **否**（挂死在 rose region_match，非 backtrack） |

---

## 3. 修复方案

### P0-1（根因，消除非确定性）：backtrack HashMap → 确定性有序结构

- `region_clue: HashMap<usize, usize>` → `Vec<Option<usize>>`（index by rid，region id 是 0..n 连续）
- `frontier: HashMap<usize, HashMap<(usize,usize), usize>>` → `Vec<BTreeMap<(usize,usize), usize>>`
- `pick_next_cell` 遍历改为 rid 升序、cell 行优先
- 影响函数：`cell_domain_size`、`frontier_assign/unassign`、`check_sealed_regions`、
  `check_edge_area_mid_search`、`check_area_lower_bounds`
- **~30-50 行，纯重构无语义变化**，可单测验证"同输入同输出"
- 预期：0749/0829/0875 要么稳定 PASS 要么稳定超时，不再随机挂死。aog 侧 HashMap 也应一并改
  （shape_digest_index 等）消除 aog 非确定性。

### P0-2（止血，让 debug 可用）：check_area_lower_bounds 打印节流

`if state.steps % 1000 == 0` 才打，或只打计数。~3 行。不修挂死，是让挂死可被观察。

### P0-3（治本，搜索空间）：见 §5 backtrack 补全

backtrack 在 area-number 大题搜索树爆炸是本质难题。消非确定后仍可能稳定超时。需补
homogeneous/differentiation/solitary 中搜索剪枝（16号 B1，~65 行）。

---

## 4. aog 的 deadline 盲区（真正的 aog 隐患）

> 虽非死循环，但单次重步可能耗时数秒，4096 粒度让超时响应慢。与 backtrack 非确定性是独立问题。

### 4.1 盲区清单（aog 侧，无 deadline 但有界）

| # | 位置 | 循环类型 | 最坏迭代 | 危险 |
|---|---|---|---|---|
| **A1** | empty.rs:328-468 `empty_area_check` | 全棋盘扫描+flood fill | O(cells²)=6.25M | **极高**（高频：dfs 每个 shape placement 调） |
| A2 | empty.rs:10-93 `dfs_empty` | 递归 flood fill | O(cells)=2500 | 高（被 A1 多处调） |
| A3 | empty.rs:767-825 `find_special_start_area` | 11 级全棋盘扫描 | 11×O(cells) | 高（每次 dfs 入口调） |
| A4 | empty.rs:97-139 `dfs_empty_compass` | 递归+compass | O(cells×compass) | 中高 |
| A5 | search.rs:653-700 `enum_tuples` | tuple 枚举 | 16384×rose_types | 中高 |
| A6 | search.rs:407-843 候选扩展 while | MAX_CAND×flood | 中 | |
| A7 | empty.rs:270-326 `ring_t_junction` | 全棋盘×4 | 中 | |

### 4.2 4096 粒度问题

`search.rs:113` 每 4096 步（stack pop 次数）查 deadline。但单次 pop 可能触发：
shape 完成 → `empty_area_check` O(cells²) + `shapes_search/insert`。4096 步 × 6.25M = 25.6B 操作，
现代 CPU 约 10-100 秒。**shape cap 后的 0606/1215 "80s 退出"即此问题**。

### 4.3 修复方案（aog deadline）

- **优先1**：`empty_area_check` 主循环 `for x/y` 每 256 格查 `core.deadline`，超时返回 false（安全回退）
- **优先2**：`find_special_start_area` 11 级链每级间查 deadline
- **优先3**：`place_non_predifined_shape` 的 4096 降至 256 或 512
- **优先4**：`dfs_empty` 递归入口每 64 格查 deadline（提前终止 flood fill，结果不完整但安全）
- **优先5**：`enum_tuples` loop 每 1024 查 deadline（跳过 slash 剪枝，安全）
- **优先6**：候选扩展循环每 64 查 deadline

**注意**：挂死根因 agent 说"不要给 empty_area_check/BF 加 deadline（非死循环）"——这是对的，
但盲区 agent 指出的是"超时响应慢"而非"死循环"。两者不矛盾：加 deadline 不是修死循环，
是让超时更快响应（从"4096 重步后"降到"256 格后"）。BF 不需加（有收敛+硬上界，单次快）。

---

## 5. 与已有文档/记忆的修正

### 5.1 需修正的记忆

| 记忆 | 误判 | 修正 |
|---|---|---|
| `optimization-progress-tracker:36` | "aog 的 empty_area_check/BF 某路径死循环" | 在 backtrack.rs:696，非 aog；非死循环，是 HashMap 非确定性+I/O 假象 |
| `compass-bbox-falsified:35` | "empty_area_check/BF 的某搜索路径死循环" | 同上 |
| `compass-bbox-falsified:35` | "1333 等题...LB: sealed 循环" | 1333=range+rose 无 area-number，不可能打印 LB:sealed；其挂死在 aog/rose |

### 5.2 白捡剩余项验证（16号）

| 项 | 状态 | 备注 |
|---|---|---|
| V3 删 regions_respect_boundaries | 仍成立 | 配合 V1 一起做 |
| D5 choose_column count==1 | **已实现**（d5-choose-column-early-break 分支） | 合入 main 即可 |
| D7 compass_rec max_sz | 仍成立 | 方向和是固有约束，非 bbox 推导 |
| D9 DLX deadline 节流 | 仍成立 | DLX 无盲区，纯性能 |
| A3 non_block LB=3 | 仍成立 | 13号 O5 数据驱动 |
| D6 DlxNode 紧凑化 | 仍成立 | 优先级低 |
| W2 Cargo profile | 部分 | panic=abort+strip 可；target-cpu=native 影响可移植性（results/bin 跨机器用） |

### 5.3 高 ROI 项在挂死修复后的价值

- **A1 K-bounding**（证伪 0 解出）：挂死修复后 K 剪枝有更多机会在超时前生效，但仍是"必要非充分"
- **B1 backtrack 补全**（4 规则剪枝）：消非确定 + 补剪枝后 backtrack 可能解出更多
- **rose K→UB / compass 全局UB**：尺寸推导，不受 compass bbox 证伪影响，仍高优先
- **enum_area_combos 惰性流式**：0882/0826/0838 OOM 根因，仍最直接

---

## 6. 推荐落地顺序（当前状态）

1. **P0-1 消除非确定性**（backtrack HashMap → Vec/BTreeMap，~30-50 行）——根因修复
2. **P0-2 打印节流**（~3 行）——让 debug 可用
3. **aog deadline 盲区**（empty_area_check 加检查 + 4096→256，~20 行）——超时响应
4. **白捡剩余**（D5 合入 + D7 + D9 + A3 + V3+V1 + W2 panic=abort+strip）
5. **尺寸界**（rose K→UB + compass 全局UB）
6. **挂死修复后重实验** A1 K-bounding
7. **enum_area_combos 惰性流式**（0882/0826/0838 OOM）

---

## 7. 新优化点（本轮）

| # | 优化点 | 位置 | 收益 | 难度 |
|---|---|---|---|---|
| **N1** | backtrack HashMap→确定性（消非确定） | backtrack.rs:637/85/108 | 0749/0829/0875 稳定 | 中（~40 行） |
| **N2** | aog empty_area_check 加 deadline | empty.rs:328 | 超时响应快（0606/1215 80s→40s） | 低（~10 行） |
| **N3** | aog 4096→256 粒度 | search.rs:113 | 超时响应快 | 极低（1 行） |
| **N4** | aog HashMap→确定性（shape_digest_index 等） | core.rs:27-28 | 消 aog 非确定性 | 中（~30 行） |
| **N5** | check_area_lower_bounds 打印节流 | backtrack.rs:696 | debug 可用 | 极低（3 行） |
| **N6** | find_special_start_area 加 deadline | empty.rs:767 | 超时响应 | 低（~10 行） |
| **N7** | 统一 deadline 工具函数 | 抽取 | 复用 | 低（~20 行） |

每项遵循 CLAUDE.md 门禁。
