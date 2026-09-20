# 27 · exact_piece_count 推导与 two-piece parity seeding 证伪

> 状态：**已证伪 / 阻塞项**，2026-09-18。
> 定位：doc 26 把 loop_closure（+3~5）与 dual_connectivity（+3~5）列为 P1，
> 两者都依赖 `exact_piece_count`。本文记录启用该字段的实验与根因，
> 结论是**当前不能启用**，两项移植被阻塞。

---

## 0. 一句话结论

`exact_piece_count` 从玫瑰窗推导（各符号类型均出现 N 次 ⇒ 恰 N 块）**计数本身正确**，
但写入该字段会自动打开 `rose.rs::propagate_parity` 的 two-piece 分支——把每条已 Cut
边种成 parity=1——该 seeding 在 1135 / 1392 上于**根节点（nodes=0）就强制出与官方解
冲突的 Cut**，属声音性 bug。计数与 seeding 在代码上不可分割（`two_piece ==
exact_piece_count == Some(2)`），因此 loop_closure / dual_connectivity 的移植被阻塞。

## 1. 实验设置

在 `edge_csp/mod.rs` 的玫瑰窗初始化处，按参考 aog
（`third_party/aog/src/solver/mod.rs:139`）推导：

```
rose_by_type 各类型格数全相等且非零 ⇒ exact_piece_count = Some(N)
```

对照组不写入（保持 `None`，即 main 现状）。**不额外加任何 parity seeding**——
2026-09-03 的历史实验（计数 + seeding 打包）已失败，本次想单独验证计数。

## 2. 实测结果（RSOLVER_TIMEOUT_MS=40000，串行）

| 题 | 规则 | 基线（计数关） | 计数开 | 判定 |
|---|---|---|---|---|
| 0213nopad | rose_window | PASS via aog | PASS via aog | 无影响 |
| 0213 | rose_window | PASS via aog | PASS via aog | 无影响 |
| 0987 | ring+fence+rose | FAIL（edge_csp 超时） | **PASS via edge_csp 6.9s** | 增益 |
| **1135** | ring+fence+watchtower+rose | **PASS via edge_csp 0.97s** | **FAIL，edge_csp 681ms 耗尽** | **回归** |
| **1392** | ring+fence+watchtower+rose | **PASS via edge_csp 14.1s** | **FAIL，edge_csp 1.9s 耗尽** | **回归** |

净 -1。1135 / 1392 的官方解均为 2 块（1135: 72+49；1392: 78+54），
计数值 `Some(2)` 本身正确。

## 3. 根因定位

1. 只有 `rose.rs:319` 读取 `exact_piece_count`（`two_piece` 标志），别处不读。
2. `two_piece` 为真时 `propagate_parity` 额外做两件事：
   - `pair_branch.diffs` 种 parity=1（根节点通常为空）；
   - **每条已 Cut 边种 parity=1**（`rose.rs:360-380`）。
3. 在 1135 上打开 `EDGE_CSP_DEBUG`：`nodes=16 unknown=132 exhausted`——
   根传播后只剩极少分支且全部矛盾。
4. 给 forcing 循环加打印（`parity FORCE Cut ... nodes=N`）：**所有强制都发生在
   nodes=0（根节点）**，例如 `e=29 cells=(101,112)`。
5. 对照官方解：cell 101=(9,2)、112=(10,2) **同属 region 0**，该边应为 Uncut。
   根层强制 Cut 直接砍掉官方解。

推导链（根层 parity=1 的来源）：同型玫瑰对（P1: (9,1)↔(10,0) 种 parity 1）+
若干 Uncut 边（parity 0）经 XOR 传递，把 (9,2) 与 (10,2) 也判成不同块。
其中必有一条 Uncut/Cut 事实与官方解不符——即 watchtower / rose 分离等前置传播
在根层留下的边状态里已有错误，two-piece parity 把局部错误放大成全局矛盾。
不开 two_piece 时同样的错误边状态只影响单条边，搜索仍能绕开（19 nodes 解出）。

## 4. 与参考实现的关系

`third_party/aog/src/solver/propagation/rose.rs::propagate_parity` 与本项目实现
逐行同构（含 Cut 边 two-piece seeding）。差异只可能来自：

- 参考的 `pair_layer.rose_by_type()` 与本项目 `rose_by_type` 的构造路径不同；
- 参考的传播调用序（watchtower / rose 分离在 parity 之前的根层状态）不同。

参考求解器自身是否也踩此坑未验证（third_party 不在本项目基准内）。

## 5. 结论与后续

- **`exact_piece_count` 保持 `None`**（main 现状，注释已更新为本文结论）。
- **loop_closure / dual_connectivity 移植被阻塞**：两者都要 `exact_piece_count`；
  在修好 two-piece parity 的声音性之前，移植它们等于把同一个 bug 接进新传播器。
- 修复方向（按优先级）：
  1. 查清根层哪条边状态与官方解不符（watchtower 传播 or rose 分离）——那是
     真正的声音性 bug，独立于 two-piece 也值得修；
  2. two-piece seeding 改为「只种同型对 + Uncut，不种 Cut 边」（牺牲强度换声音）；
  3. 或让 `exact_piece_count` 与 two-piece seeding 解耦（新字段
     `piece_count_for_loop_closure`），供 loop_closure 单独使用。

## 6. 关联

- doc 26 §2.1 / §2.2：loop_closure、dual_connectivity 方案（被本文阻塞）
- `rsolver/src/solver/edge_csp/rose.rs::propagate_parity`
- `rsolver/src/solver/edge_csp/mod.rs` 玫瑰窗初始化处的 NOTE 注释
