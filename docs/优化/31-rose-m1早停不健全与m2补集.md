# 31 — rose `region_match` 的 M1 早停不健全 + m==2 补集封面

日期：2026-09-21
基线：1160/1258（`0d326d2`）

## 0. TL;DR

rose 求解器对 14 道**有官方解**的题报告 `exhausted`（"无解"）——又一类
soundness bug，和 doc 29/30 是同一种签名。根因是
`region_match::generate_all_candidates` 的 **M1 早停**：

> 一旦候选集集齐了所有符号类型就记录并**停止扩张**（"更大的超集用掉更多格子、
> 同样的符号，对精确覆盖严格更差"）。

这个论证只在「最小完整集已经够大、能进某个精确覆盖」时成立。当尺寸窗口强制区
域**大于**最小符号张成集时，早停把所有可用候选都丢了，`region_match` 返回假
"无解"。

1333（7×7，5 区域，`range max 10`）就是典型：49 格、5 区域、每个 ≤10，唯一能
凑出 49 的尺寸多重集是 `{9,10,10,10,10}`，而每个最小 P1+P2 张成集只有 5–8 格。
5 个 area combo 全枚举出来了，`candidates_by_size` 里却没有 size 9 或 10 的候
选，319ms 报无解。

## 1. 尺寸窗口

一个大小为 `s` 的区域要能出现在 `total` 格、`m` 个区域、每个在
`[min_sz, max_sz]` 的精确覆盖里，必须让剩下的 `total - s` 格还能被拆成 `m-1`
个合法区域：

```
useful_min = max(min_sz, total - (m-1) * max_sz)
useful_max = min(max_sz, total - (m-1) * min_sz)
```

1333 上算出 `[9, 10]`。M1 的早停只在 `current.len() >= useful_min` 之后才允许，
于是 9 格的完整集会挡住长成 10 格区域的那一步——**早停必须延到 `useful_max`，
不是 `useful_min`**。

## 2. 为什么不能无条件延到 `useful_max`

0833（10×11、54 格可填、6 区域、3 符号类型、无尺寸约束）的窗口是 `[1, 49]`。
把早停整个去掉会让候选 BFS 一路长到 49 格——这正是历史上
「M1 revert 让 0833 从解出变不可解」的原因。

所以按**窗口宽度**分流：

```rust
pub const WINDOW_GROW_LIMIT: usize = 8;
let grow_through = useful_max.saturating_sub(useful_min) <= WINDOW_GROW_LIMIT;
if complete_multi && !grow_through && current.len() >= useful_min { continue; }
```

- 窗口窄（≤8）：尺寸约束在真正起作用，长到 `useful_max`。1333 窗口宽 1。
- 窗口宽：尺寸约束松，最小集已经可用，保留 M1 早停。0833 窗口宽 48。

**这是和 M1 同一性质的启发式**（窗口宽且区域又必须远大于最小集时仍会漏候选），
不是完全的健全性。彻底健全要一路长到 `useful_max`，代价就是 0833 那类爆掉。

## 3. `m == 2` 的补集封面

只有两个区域时它们平分 `all_positions`，所以 seed 0 的任何完整候选 `A` 的补集
`all_positions \ A` 就是第二个区域**唯一可能的形状**。BFS 从不生成这些补集
（它们远大于最小符号张成集，M1 早就停了），于是 2 区域的 rose 题连一对候选都凑
不出来。

`try_complement_cover`：对 seed 0 的每个完整候选做一次补集连通性 BFS，通了就是
一个精确封面。符号覆盖不用查——`A` 已经集齐每种类型各一个，盘面上每种恰好
`m == 2` 个，补集自动拿到另一个。每个候选一次 BFS，最坏 20000 × O(格) 量级，
毫秒级。

## 4. 这轮没救回来的：候选生成饿死

0974（12×12、4 符号类型 × 2、`ring` 预切 46 条边）是
`rose: seed 52 -> 0 candidates in 1.57s`——**一个候选都没生成**。BFS 按尺寸
广搜，最近的异类符号离 seed 约 6 格，要长到 7 格才集齐 4 类，而 144 格上尺寸
≤7 的连通集数量级早就撞上 `VISITED_CAP = 2M`。补集封面帮不上忙，因为
`cands0` 是空的。

这是 memory 里记的「rose 候选爆炸 / 范式错配」，不是剪枝问题。0974 及同类
（0382 / 0960 / 0987 / 1137 / 1149a / 1249 等 m==2 大盘）要靠 doc 20 的
P2 边传播范式迁移，不是这里能修的。

## 5. 实测

`--rules rose_window` 子集（171/187 有记录）：**+2（0826、1333），0 回归**。
0833 仍 via rose 解出，且 rose 自身耗时 7.96s → 3.8s（窗口计算把无用的超大候
选挡在了外面）。

**全量基准 `--timeout 40 -j 6`：1160 → 1162 / 1258**，新解
**1333（via rose，本条的直接收益）/ 0990（via edge_csp）/ 0745（via pieces）**。
1140fix 在 `-j 6` 下争抢翻负，**串行复测 62.7s SOLVED via edge_csp**（噪声，
与 doc 30 记的同一条）。

`AOG_DEBUG=1` 下新增两条可读诊断：

```
rose: size window [9, 10] (min=1 max=10 total=49 m=5)
rose: seed 0 sizes [(5, 1), (6, 15), (7, 87), (8, 364), (9, 1314), (10, 4326)]
rose: m==2 complement cover found
```

全量基准见 `results/bench/20260921_<sha>_rose-m1.{txt,jsonl}`。

## 6. 追加：`m == 2` 补集必须过验证器

PR #78 的 `try_complement_cover` 只检查补集**连通性**就连着返回了第一个，但环顶
点度 / inequality / watchtower 这些规则它一概没看。1137
（`ring + inequality + watchtower + rose_window`，9×11）上就是
`rose: m==2 complement cover found` → 54ms `validation_failed` → 整条
`region_match` 路径被放弃。

改成：每个补集都跑一遍完整验证器，**只返回第一个通过的**；一个都不通过就返回
`None`，让调用方继续走正常搜索。代价是最坏 20000 次 `validate`（1137 上约
600ms），所以先用题目自身的 `[min_sz, max_sz]` 把尺寸不合法的补集挡掉再验证。

`--rules rose_window` 子集（187 道）与 PR #78 的结果**完全持平**（155 PASS /
32 FAIL，零增零损）——这条是纯粹的正确性修复：在「第一个连通补集不合法、但后面
某个合法」的题上它才兑现，目前语料里没有这样的题，但不修就是一颗埋着的假接受。
