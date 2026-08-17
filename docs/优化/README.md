# rsolver 内存性能优化——调研与方案系列

> 状态：**调研/方案**文档，尚未改动任何求解代码。
> 适用范围：Rust 求解器 `rsolver/`（内存方向）。Python 求解器内存问题见
> `docs/重构/data-structures.md`（Cell 192B 等建模对比）。
> 关联源码：`rsolver/src/**`；关联代码详解：[rust-solver 文档系列](../rust-solver/README.md)。
> 实测基线：`results/20260806_内存基线调研.md`。
>
> 更新日期：2026-08-06

---

## 0. 一句话结论

`rsolver` **没有经典意义上的 Rust 内存泄露**（无 `Rc` 循环、无 `Box::leak`），但有
**两个会造成多 GB 内存占用的无界增长热点** 和 **一批固定的常驻浪费**：

| 严重度 | 问题 | 量级 | 触发条件 |
|---|---|---|---|
| 🔴 P0 | **aog 自由形状搜索的形状库无界增长** | **5~7 GB** | 无尺寸/形状约束题（watchtower/ring 等）且 aog 无法在 30s 内解出 |
| 🔴 P0 | **rose region_match 候选 `visited` 无上限 + 多层克隆** | **~100 MB** | 玫瑰窗题每 seed 撞 20000 候选上限（如 C4-2） |
| 🟠 P1 | **aog `Pools::new(100)` 预分配 100 层定长数组** | **~3.6 MB 常驻** | 所有走 aog 的题，即使 6×6 小盘 |
| 🟡 P2 | backtrack `HashMap` 状态、DLX 节点、`Cell` 结构体臃肿 | KB~MB 级 | 各求解器路径 |

内存实测曲线：aog 自由形状搜索在 8×10 盘上 **~600 MB/s 线性增长**，30s 内可到 6.8 GB+，
跑完后 RSS 不回降（形状库全程驻留）。

---

## 1. 文档地图

| 文档 | 内容 | 读完能回答 |
|---|---|---|
| [01-内存现状与实测基线](01-内存现状与实测基线.md) | 各求解路径内存实测数据 + 内存去向归因表 | 内存都花在哪了？ |
| [02-根因分析](02-根因分析.md) | 每个问题的机制、代码位置、与 C++ 参考的差异 | 为什么会涨到 5GB？ |
| [03-优化方案](03-优化方案.md) | 分优先级方案 + 每项的风险/工作量/行为影响 | 怎么改？改完会怎样？ |
| [04-内存泄露审计](04-内存泄露审计.md) | Rust 语义泄露审计 + 分配器效应 + 不释放问题 | 到底有没有内存泄露？ |
| [05-实施路线图](05-实施路线图.md) | 分期实施顺序 + 门禁要求 + 验证方法 | 先做哪个？怎么验收？ |
| [06-图论与数学规划优化方法](06-图论与数学规划优化方法.md) | 图论方法（着色/匹配/因子/割空间）的实践方案 | 哪些图论算法能加速特定规则？ |
| [07-数学规划建模分析](07-数学规划建模分析.md) | ILP/CP/SAT/SMT 逐规则编码分析与可行性 | 数学规划能建模哪些子问题？为什么整体替换不行？ |
| [08-其他数学方法调研](08-其他数学方法调研.md) | 拉格朗日松弛、MCTS、谱序、熵序、对称破缺、次模优化、拓扑 | 图论和数学规划之外，还有哪些数学方法可用？ |
| [09-rose-puzzle-piece优化调研](09-rose-puzzle-piece优化调研.md) | 拼块(puzzle_piece)规则 171 题分类、求解瓶颈与 5 个优化方向 | 拼块规则如何优化求解？ |
| [10-专用求解器方案](10-专用求解器方案.md) | 针对 fence/ring/rose+same 等 7 种规则组合的专用求解器设计与实施路线 | 哪些规则组合需要专用求解器？怎么设计？ |
| [11-求解器优化理论总纲](11-求解器优化理论总纲.md) | 数据结构/算法理论、剪枝技巧、求解范式对比、业界做法、参考求解器未吸收技术、综合路线图 | 求解器为何这样设计？还能从哪些理论方向优化？哪些已证伪？ |
| [12-优化项价值评估与路线图修订](12-优化项价值评估与路线图修订.md) | 4 agent 对 19 优化项的量化收益交叉验证、去重、依赖图、修订后全局排序 | 各优化项实际能解几道题？哪些已落地/价值清零？先做哪个？ |
| [13-官方语料二级结论](13-官方语料二级结论.md) | 官方 1229 题归纳的 15 条规则语义结论 + 14 条求解器可用二级结论（按价值排序），附验证数据、compass 面积界推导、5 条解析陷阱、已证伪清单 | 官方题有哪些可利用的统计规律？compass 面积界怎么算？哪些归纳陷阱曾导致假结论？ |
| [14-边变量CSP独立求解器方案](14-边变量CSP独立求解器方案.md) | 架构不同→独立求解器决策（非嵌入aog）、完整~8000行移植范围、内部三态Edge、混合前置+后置路由、SolitaryGrower证伪同步、分阶段实施 | 为什么不嵌入aog？新求解器移植多大？放路由链哪？ |
| [15-求解器优化新发现](15-求解器优化新发现.md) | 第二轮4维度22个新优化点：7白捡项(env缓存/编译profile/check_radar守卫等)、3高ROI算法(K-bounding 85题/惰性组合/多符号剪枝)、compass连通性下界闭式解、rose伴生规则28 NOSOL根因、2否定假设 | aog常数因子能怎么压？K为何没用？rose NOSOL根因？ |
| [16-求解器优化新发现第二轮](16-求解器优化新发现第二轮.md) | 第三轮4维度22个新优化点：validate region_of O((HW)²)→O(1)、DLX row_check未用/附加列锚定、aog尺寸界没用K/compass LB(rose OOM根因)、backtrack缺4规则剪枝、参考求解器小技术细节(loop_closure/delta_gemini/ParityUF) | validate为何慢？DLX基础设施闲置？aog尺寸界为何松？ |
| [17-挂死根因与deadline盲区](17-挂死根因与deadline盲区.md) | "LB:sealed"根因(backtrack HashMap非确定,非aog死循环)、deadline盲区(empty_area_check无deadline+4096粒度)、7新优化点(HashMap→Vec/aog deadline/统一工具)、修正记忆误判 | 挂死真正根因？deadline为何不触发？aog盲区在哪？ |
| [18-第五轮调研-edge-csp第二迭代与剩余FAIL深挖](18-第五轮调研-edge-csp第二迭代与剩余FAIL深挖.md) | 3 agent 深挖：19道OOM是配置非算法(DEFAULT_SHAPE_CAP=0+preempt未接线)、rose 50道缺"伴生规则剪枝"债S1、fence 38道需 palisade 传播迁入 edge_csp；edge_csp 第二迭代路线(内部验证→compass→watchtower→differentiation/boxy) | OOM 为何是配置问题？rose 最大缺口？edge_csp 第二迭代怎么做？ |
| [19-类型题专用求解器方向](19-类型题专用求解器方向.md) | 2 agent 重扫 22 规则×6 求解器覆盖矩阵：唯一值得新建专用求解器的是形状同一性三兄弟 same/different/mixed(~21道，仅 same 有参考 solve_match 可移植)；homogeneous/non_block 落点扩 edge_csp+aog 补丁 | 还有哪些规则需要专用求解器？same/different/mixed 怎么解？ |
| [20-第六轮调研-rose与compass核心算法重构](20-第六轮调研-rose与compass核心算法重构.md) | 2 agent+参考求解器实测：rose 与 compass 收敛到 edge_csp 边传播宿主；参考 pair.rs 4/6 纯 rose 秒解(0-0.4s vs 本项目40s超时)；compass"桥/网关强制"是最高ROI最小改动 | rose/compass 为什么要范式迁移而非独立求解器？最高ROI改动是什么？ |
| [21-剩余FAIL硬度分类与优化天花板](21-剩余FAIL硬度分类与优化天花板.md) | 第七轮综合(capstone)：186 FAIL 五档分类(①配置bug19 ②传播缺口~25 ③范式错配rose55+fence ④形状同一性~21 ⑤根本难~40)；PASS 天花板 ~1120-1160；P0-P6 路线图；watchtower 双触碰语义补挖 | 还剩什么、能解多少、按什么顺序做？优化天花板在哪？ |
| [22-数学建模专用求解器方向](22-数学建模专用求解器方向.md) | 2 agent：可建模 10 条规则(ring/brick/watchtower/compass/area/precise/range/difference/inequality/fence)；正确形态=edge_csp 传播内核全局代数补全(~950行)非独立模块；SAT/ILP 不划算(连通 O(HW²)) | 哪些题型可数学建模？为什么不做独立 SAT/ILP 求解器？ |
| [23-求解器返回信息完善方案](23-求解器返回信息完善方案.md) | 求解结果透出 per-module 求解器清单/耗时/失败原因：Rust Solution 加 attempts[SolverAttempt{status,elapsed_ms,note}]，JSON→Python→UI/benchmark 三层，ModuleOutcome 枚举替代 Option 返回，失败六态分类 | 每道题各求解器各耗时多少？为何失败(超时/无解/校验失败)？由谁解出？ |

---

## 2. 关键数字

- **P0 止血点**：aog 形状库上限 / rose `visited` 上限——两者改动都只影响「超限后该分支提前放弃」，
  正确性由路由链（pieces/backtrack）+ 独立验证器兜底，但 **可能改变可解性**，需按门禁跑全量回归。
- **P1 白捡**：`Pools` 惰性扩容——**纯内存优化、行为等价**，不碰搜索语义，风险最低，建议最先落地。
- **官方题库影响面**：2488 道官方题中 **1649 道（66%）无尺寸/形状约束**，其中无法在 30s 内由
  aog 解出的会出现内存爆炸（多数小盘 / rose 题可秒解，但病态样例已实测 5~7 GB）。

---

## 3. 与 C++ 参考实现的关系（重要）

- C++ `AoG_Solver` 的 `shapes` 同样是**无界** `std::vector<Shape>`，`node_to_shape_index` /
  `shape_digest_index` 同样持续累积——**形状库无界增长是参考实现自身缺陷**，Rust 忠实移植复刻。
- C++ 的 `mark_skip_shape[MARK_SKIP_CAP=262144]` 是固定数组：当形状数超 26 万时 C++ **越界读**
  （未定义行为）；Rust 用 `Vec` 安全实现，不崩溃，但把「越界」变成了「内存继续增长」。
- C++ 求解器**没有 deadline**，靠外层 batch 脚本 kill；Rust 移植加了 30s deadline（search.rs Fix B/C），
  但在 swap 抖动下无法及时停住。**优化不违反移植忠实性——这是对参考实现缺陷的修复。**
