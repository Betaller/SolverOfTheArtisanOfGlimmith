# 优化方向 · 分方向详细文档索引

> 本目录是 `docs/优化/24-优化方向总览与方法全景.md` 的**展开**。24 篇把全部方向归并成状态地图 + 补遗；本目录对**每一个方向**给一份独立详述：思想 / 收益 / 代价 / 优先级 / 实现思路 / 验证方法 / 依赖 / 参考。
> 命名：`NN-kebab.md`，`NN` 为全局序号（非优先级）。状态图例：🟢新方向 / 🟡待办（高ROI）/ ✅已落地 / 🔶已证伪 / ⚪低优先级。

## 并行化（§4）
| 文件 | 方向 | 状态 |
|---|---|---|
| [01-parallel-inter.md](01-parallel-inter.md) | 模块间并发「先赢取消」 | 🟢新 |
| [02-parallel-intra.md](02-parallel-intra.md) | 模块内并行 DFS / work-stealing | 🟢新 |
| [03-parallel-puzzle.md](03-parallel-puzzle.md) | 谜题间并行 + 子进程复用 | ✅部分落地 |
| [04-parallel-gpu.md](04-parallel-gpu.md) | GPU offload（远景） | ⚪低优先级 |

## 机器学习 / 学习启发式（§5）
| 文件 | 方向 | 状态 |
|---|---|---|
| [05-ml-routing.md](05-ml-routing.md) | 学模块路由策略 | 🟢新 |
| [06-ml-ordering.md](06-ml-ordering.md) | 学变量序 / 值序 | 🟢新 |
| [07-ml-nogood.md](07-ml-nogood.md) | 轻量冲突学习 / nogood | 🟢新 |
| [08-ml-restart.md](08-ml-restart.md) | 随机化重启 | 🟢新 |

## 底层系统调优（§6）
| 文件 | 方向 | 状态 |
|---|---|---|
| [09-low-hashset.md](09-low-hashset.md) | 消除 backtrack 热路径 HashSet 分配 | 🟢新 |
| [10-low-bitvector.md](10-low-bitvector.md) | 区域位向量 / SIMD | 🟢新 |
| [11-low-doublevalidate.md](11-low-doublevalidate.md) | 去双重 validate | 🟢新 |
| [12-low-allocator.md](12-low-allocator.md) | 分配器 / 编译选项 | 🟢新 |

## 实验方法论（§7）
| 文件 | 方向 | 状态 |
|---|---|---|
| [13-meta-rustparts.md](13-meta-rustparts.md) | RUST_PARTS 墙钟放大修正 | 🟢新 |
| [14-meta-determinism.md](14-meta-determinism.md) | 确定性 / 可复现 | 🟢新 |
| [15-meta-evalproto.md](15-meta-evalproto.md) | 评估协议 | 🟢新 |

## 进一步全新方向（§11，N1–N14）
| 文件 | 方向 | 状态 |
|---|---|---|
| [16-incr-solve.md](16-incr-solve.md) | 增量 / 交互式求解（UI） | 🟢新 |
| [17-fast-codec.md](17-fast-codec.md) | 更快序列化协议 | 🟢新 |
| [18-incr-validate.md](18-incr-validate.md) | 增量验证 | 🟢新 |
| [19-simd-flood.md](19-simd-flood.md) | 位并行 flood-fill | 🟢新 |
| [20-active-decomp.md](20-active-decomp.md) | 主动降维分治 | 🟢新 |
| [21-deduction-engine.md](21-deduction-engine.md) | 预搜索 deduction engine | 🟢新 |
| [22-relax-warmstart.md](22-relax-warmstart.md) | 松弛求解 warm-start | 🟢新 |
| [23-walksat.md](23-walksat.md) | WalkSAT 快路径 | 🟢新 |
| [24-distributed.md](24-distributed.md) | 跨机 / GPU 穷举 | 🟢新 |
| [25-diff-fuzz.md](25-diff-fuzz.md) | 差分 / fuzz 验证 | 🟢新 |
| [26-puzzle-jit.md](26-puzzle-jit.md) | 编译式特化（puzzle-JIT） | 🟢新 |
| [27-corpus-cache.md](27-corpus-cache.md) | 语料记忆化 / 解缓存 | 🟢新 |
| [28-ml-deep.md](28-ml-deep.md) | GNN / RL / Transformer 深化 | 🟢新 |
| [29-adapt-budget.md](29-adapt-budget.md) | 自适应预算 | 🟢新 |

## 续 · 更多新方向（30–37，第二轮新调研补充）
> 以下 8 条是 24 的 §4–§7 与 §11 **均未覆盖**、本文第二轮调研补充的方向。

| 文件 | 方向 | 状态 |
|---|---|---|
| [30-portfolio-config.md](30-portfolio-config.md) | 参数多样化组合（配置级 portfolio） | 🟢新 |
| [31-feasibility-memo.md](31-feasibility-memo.md) | 子问题可行性记忆表 | 🟢新 |
| [32-cutset-decomp.md](32-cutset-decomp.md) | Cutset / 关节变量分解 | 🟢新 |
| [33-hybrid-outsource.md](33-hybrid-outsource.md) | 子问题外包给专用引擎 | 🟢新 |
| [34-trace-tooling.md](34-trace-tooling.md) | 搜索过程追踪与分析工具（元方向） | 🟢新 |
| [35-check-ordering.md](35-check-ordering.md) | 约束检查排序 / fail-fast | 🟢新 |
| [36-anytime-partial.md](36-anytime-partial.md) | Anytime 渐进解 / 部分解返回 | 🟢新 |
| [37-derived-constraints.md](37-derived-constraints.md) | 规则组合派生约束引擎 | 🟢新 |

> 总入口与分类地图见 `../24-优化方向总览与方法全景.md`。每篇末尾「参考」回链到 24 的对应小节与 `docs/优化/` 前 23 篇（如已存在专文）。
