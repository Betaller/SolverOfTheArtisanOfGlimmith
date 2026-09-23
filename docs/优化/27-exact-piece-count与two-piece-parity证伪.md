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
  （dual_connectivity 后来改挂 `structural_pieces` 落地，见 doc 12/15。）
- 修复方向（按优先级）：
  1. 查清根层哪条边状态与官方解不符（watchtower 传播 or rose 分离）——那是
     真正的声音性 bug，独立于 two-piece 也值得修；
  2. two-piece seeding 改为「只种同型对 + Uncut，不种 Cut 边」（牺牲强度换声音）；
  3. 或让 `exact_piece_count` 与 two-piece seeding 解耦（新字段
     `piece_count_for_loop_closure`），供 loop_closure 单独使用。

### 5.1 补充实测（2026-09-20）：无 vertex 线索也翻车

在 **0974**（Zone3/2-loopy，`ring+rose_window`，12×12，官方 2 块，**无任何
vertex/watchtower 线索**）上启用计数（`exact_piece_count = rose_structural_pieces`，
条件为“无 watchtower 线索”）：

| 配置 | edge_csp 结果 |
|---|---|
| 计数关（main） | nodes=1173，40s 超时 |
| 计数开 | **nodes=34，1.8s exhausted（错剪）** |

**结论修正**：two-piece parity seeding 的不可靠**不依赖** watchtower 在根层留下的
错误边状态——0974 上根本没有 vertex 线索，seeding 单独把官方解剪掉了。
§3 的“上游错误边状态放大”解释不完整；seeding 本身（同型对 parity 1 + Cut 边
parity 1 的 XOR 传递）在这套传播调用序下就是不可靠的。方向 1（查上游）优先级
下调，方向 2/3（削弱或解耦 seeding）成为主线。

同日尝试了方向 3 的变体：移植 `propagate_loop_closure` 挂到 `structural_pieces`
（与 parity 完全解耦，绕开 seeding）。0974 上 nodes 1173→692，仍超时；单独收益
不足，随工作树回退。移植要点存档：`max_loops = pieces_cap - 1`，DSU 数 Cut 边
连通分量的奇度顶点分类 loop/open，`num_loops > max_loops` 即矛盾；边界 Uncut 与
单环饱和两条强制规则需 `max_loops == 1` 且仅对 loopy 可靠。重启时可按此重建。

## 6. 关联

- doc 26 §2.1 / §2.2：loop_closure、dual_connectivity 方案（被本文阻塞）
- `rsolver/src/solver/edge_csp/rose.rs::propagate_parity`
- `rsolver/src/solver/edge_csp/mod.rs` 玫瑰窗初始化处的 NOTE 注释

---

## 7. 更新（2026-09-22）：证伪前提已愈，`exact_piece_count` 重新启用

§3 推导链的结论是「two-piece parity 把**根层已有的错误边状态**放大成全局矛盾」——
错误边状态才是真凶，parity 只是放大器。此后落地的修复（3262e5d watchtower Pass B
对角格误判、陈旧组件快照、bricky_loopy 赌博剪枝）把根层错强清除后，放大器不再有
可放大的错误：

- 1135 / 1392（doc §2 的两道回归）：**0.3s / 1.3s SOLVED**，回归消失；
- 0987（doc §2 的增益题）：40s 超时 → **7.4s SOLVED**，§2 预言兑现；
- 0974（§5 提及的根层错杀）：真凶实为 **rose pair 分支的不完备 witness 路径强制 +
  `pair_branch.diffs/sames` 快照泄漏**（见官方题状态文档 2026-09-22 条目），修复后
  **13s SOLVED**——与 two-piece parity 无关。

**处置**：`edge_csp/mod.rs` 按 aog 规则重新推导（rose 各类型格数全相等 n ⇒ `Some(n)`）；
本文状态由「已证伪/阻塞项」改为「**历史证伪，前提已愈，已重新启用**」。loop_closure /
dual_connectivity 的阻塞解除（dual 已挂 `structural_pieces` 先行落地）。

**仍然成立的教训**：parity seeding 的声音性依赖上游传播的根层状态；任何「全局放大器」
类传播（parity/UF/传递闭包）出错时优先查**被放大的局部事实**，而不是放大器本身。

**2026-09-22 追记**：exact_piece_count 解锁后 loop_closure（doc 26 §2.1 P1）已移植
（仅规则 1+4；参考的 `@` 度数规则与我们的 watchtower 语义不符，官方解实测 241/471
反例，弃）。望塔精确度判死传播同日落地并解出 1137；其**强制分支存在未明交互 bug
（singleton fit 钉边杀真解）**，已留空待查——判死部分健全且已足够。

**2026-09-23 追记（wdegree 交互侦查，未结案）**：

1. **强制模型已证健全**：val→割度四前提（1→0、2→{2,4}、3→{3,4}、4→4）对 1568 个
   官方满象限顶点 **0 反例**；两条塌缩前提同样干净（m=2 val=2 → deg==2：634/634；
   ring 顶点无奇割度：587/587，含边界顶点）。纸面推导与官方数据一致，强制规则
   本身无罪。
2. **追踪法陷阱（复现者必读）**：给 `set_edge` 加 `#[track_caller]` 写入追踪时，
   `self.probe(|s| s.set_edge(...))` 沙箱里的**试探性假设**也会被记成"写入"。
   由此曾误判"根层存在错误强制"——按调用行过滤试探写入后，1135 根层演绎写入
   与官方解完全一致。判定演绎错误必须先排除试探。
3. **放大器结构与本文 §3 同型**：singleton-fit 强制把输入边状态里的单点错误
   放大成全局矛盾。剩余嫌疑：probe 沙箱 `restore()` 后的**陈旧组件缓存**——
   `snapshot()`（`edge_csp/mod.rs`）只回滚 `edges`/`changed`/`pair_branch`，
   `curr_comp_id` / `curr_comp_sz` / `growth_edges` 不在快照内，沙箱内
   `build_components` 的产物会泄漏给恢复后的传播轮（"陈旧组件快照"类第三次
   出现：S3/假 cc、D0 误冻结同源）。
4. **同日深挖（假设迭代三轮，均未完全结案）**：
   - restore() 后强制重建组件缓存 + 开强制 → 三题仍杀（stale-comp 假设不足）；
   - **深度标记实测：根层 24 条强制 24 条正确**（对照官方解 0 反例），沙箱内
     fits 空的 Err 仅 1 次且是合法条件矛盾（kc=3>2）——**演绎层完全无罪**；
   - Uncut-only 非对称消融（Cut 强制全关）同样杀三题（0.3s 解 → 40s 磨满超时，
     连假穷尽都不是）；probe 关掉也不救——**杀解在搜索机制层**，与 doc 27 §3
     的 parity 放大器不同型；
   - 剩余假设排序：①set_edge 写流量暴露的搜索态记账缺口（snapshot/restore
     对 Solver 全字段的水位线审计）；②强制改变 select_edge/pair-branch 树形后
     触发的病态搜索路径。四象限顶点 val→割度模型、`cell_pair_indices` 与
     `vertex_cells` 行主序匹配、`flood_fill_decided` 只漫 Uncut——均逐一验证无误。

---

## 8. 结案（2026-09-23 晚）：真凶＝陈旧组件缓存的卡口假强制，wdegree 强制已解锁

第 4 条假设排序里的「记账缺口」方向反了——**回滚机制无罪，缺的是消费端的重建**。
完整因果链（逐层实证）：

1. `build_components` 在洪水之后写 growth-edge 切割，`propagate_area_constraints` /
   watchtower Pass A / wdegree 强制又在轮内继续写边——**组件缓存在轮内即刻陈旧**。
2. `propagate_rose_separation` 的卡口推理（Phase 1 chokepoint）从陈旧 `comp_cells`
   做可达性 BFS：低估「缺型可达性」→ 假卡口 → **假 Uncut 强制**（实测首错：
   1135 的 (6,4)-(7,4) 官方 Cut 被钉 Uncut，`rose.rs` chokepoint 强制点）→
   级联至 watchtower Pass A / bricky 假强制 → 根层矛盾 → 秒退。
3. **wdegree 强制只是放大器**：它加大轮内写流量、拉宽陈旧窗口，让卡口假强制
   必然触发——这解释了「强制开就杀、关就好」的全部现象（演绎本身 24/24 正确）。
4. 修复：`propagate_watchtower` / `propagate_rose_separation` /
   `propagate_rose_phase3` 入口各自 `build_components()`（`build_components`
   转 `pub(crate)`）。修复后 wdegree singleton-fit 强制**安全启用**：
   1135 36ms / 1392 414ms / 1137 24s 全 SOLVED（1137 较强制关闭时的 29s 更快），
   回归点 1294/1017/0987/1378/1110 全绿。

**通用教训（第四次陈旧组件事故）**：组件缓存是「写边即失效」的派生数据，
**每个读它的传播器都必须自带重建（或显式延迟到下一轮）**——dual 的延迟、
doc30 的 progress 重建、本次的消费端重建，都是同一规则的实例。谁读谁重建。
