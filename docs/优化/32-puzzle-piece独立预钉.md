# 32 — `shape_pattern` 独立预钉（非 rose 的 puzzle_piece 前置）

日期：2026-09-21

## 0. TL;DR

11 道 `puzzle_piece` FAIL 里，0976 / 0606 / 1215 的结构是
**N 个 shape_pattern 区域 + 1 个无约束大区域**（例如 0976：11×10、5 个 6 格
pattern 区域 + 1 个 80 格区域）。这类题在原路由下**没有任何求解器能尝试**：

| 求解器 | 为什么不行 |
|---|---|
| aog | 形状库 OOM（exit -9），200k cap 重试也解不出 |
| rose | 没有 `rose_window` 规则，`rose_capable=false`，根本不跑 |
| edge_csp | `puzzle_piece` 被 `is_edge_csp_capable` 故意排除（见下） |
| pieces | DLX 用 shape_pool 铺盘，**没有"一个大无约束区域"的概念**，0976 2ms exhausted |
| backtrack | `rules` 非空 → 默认禁用 |

把 `rose::puzzle_piece_pin` 的预钉逻辑提成独立前置 `solve_puzzle_piece_standalone`，
在 edge_csp 之前对「有 `puzzle_piece` 且无 `rose_window`」的题跑一遍：枚举每个
`shape_pattern` 锚点的二面体放置 → 求两两不相交的完整指派 → **把剩余所有格子当成
一个区域** → 过完整验证器。三道题全部解出。

## 1. 为什么不能让 edge_csp 容忍 `puzzle_piece`

先试的是把 `puzzle_piece` 加进 edge_csp 的 `SUPPORTED` 容忍集——**0 新解**。
`puzzle_piece` 是"区域形状 == 某个 pattern"的约束，边变量范式里没有形状变量，
只能靠叶节点 `validate` 过滤，40s 全烧在盲搜上；0606/1215 还直接被 OOM kill。
`mod.rs` 里那段排除注释是对的，已回退并记进 memory。

## 2. 实现

`rsolver/src/solver/rose/puzzle_piece_pin.rs` 新增
`solve_puzzle_piece_standalone` + `combine_plain`。复用已有的
`enumerate_pin_candidates`（传空 `symbol_types` 即跳过 rose 的符号平衡过滤），
但**不能**复用 `enumerate_pin_assignments`：它的 `combine` 会 `rem[0]`，空符号表
直接 panic。

`combine_plain` 是逐锚点的不相交放置回溯；每到一个完整指派就把剩余格子标成
一个新区域并立刻 `validate`，通过就返回。**错误的剩余区域会被验证器挡掉**，
所以不存在假接受；搜不到就返回 `None`，后面的链路照旧。

范围限制（都靠 fall-through 兜底，不引入不健全）：

- 一个区域里放多个 `shape_pattern` 格（0493：11 个 pattern 格但只有 8 个区域）
  ——不相交约束会排除正确指派，搜不到即放弃。
- 剩余部分本身要再分成多个无约束区域（0994：10 pattern / 15 区域；
  1435：41 pattern / 51 区域）——本实现只试"剩余 = 一个区域"。

## 3. 两个实施陷阱

**（a）deadline 锚点**：`solve_puzzle_piece_standalone` 必须锚到**自己的**
`Instant::now()`，不能用调用方的全局 `start`。aog 到这里通常已经烧完整个 unit
预算，用全局起点的 deadline 等于已经过期、立刻返回 None——和
`solve_rose` 开头那段注释记的是同一个坑。

**（b）预算**：给完整 unit 预算，不要砍。1215 的放置搜索要 ~36s（40s 预算），
砍到 3/5 就搜不完。墙钟 `timeout × RUST_PARTS × SLACK` = 40 × 4 × 1.2 = 192s
足够覆盖 aog(40) + pp-pin(40) + pieces(40) 且给 OOM 重试留了余量。

## 4. 实测

三道题全路由（带 `AOG_SHAPE_CAP=200000` 模拟 OOM 重试）：

| 题 | 结果 |
|---|---|
| 0976 | **PASS via pp-pin**（aog 40s 超时 + pp-pin 10.3s） |
| 0606 | **PASS via pp-pin**（aog 40s + pp-pin 0.25s） |
| 1215 | **PASS via pp-pin**（aog 40s + pp-pin 36.0s） |

## 5. 全量基准与噪声

`--timeout 40 -j 6`：**1162 / 1258**（上一轮 1165）。表面上是 −3，实际是：

- **新能力 +3**：0606 / 0976 在并行下也解出，1215 因 pp-pin 需要 36s 的 40s 预算、
  `-j 6` 下被挤爆而未进榜——**三道题串行复测全部 PASS via pp-pin**
  （40.4s / 50.7s / 74.0s）。
- **噪声 −5**：0418 / 0685 / 0745 / 0826 / 0990 全是历史争抢噪声题，上一轮翻正、
  这一轮翻负，串行复测全部 PASS（66.9s / 20.7s / 68.1s / 14.6s / 73.3s）。

所以 **`-j 6` 口径的噪声带至少 ±5**，1165 那个数字里本来就含 3 道噪声翻正。
稳定能力线是 **1162 + 3 = 1165**。判能力变化只能靠串行复跑，不能看单次全量数字。
