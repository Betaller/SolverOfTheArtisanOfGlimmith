# 28 · GNN / RL / Transformer 深化

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N12）｜ 来源：`docs/优化/24` §11.13
> 关联：[05-ml-routing.md](05-ml-routing.md) · [06-ml-ordering.md](06-ml-ordering.md) · [15-meta-evalproto.md](15-meta-evalproto.md)

## 1. 一句话
深化 §5 的 ML 方向：① **GNN** over grid graph 预测区域连通性/形状类别；② **RL（PPO）** 学 DFS 长程分支策略（比梯度提升更适配信用分配）；③ **Transformer** 把「谜题 → 模块执行序列」当序列决策。

## 2. 思想（为什么有效）
- **GNN**：谜题天然是**图**（格为节点、邻接/边界为边，节点带线索特征、边带 cut/uncut 状态）。区域划分本质是图上的**节点聚类**。GNN 能学习「哪些格应属同一区域」的表示，输出可作：候选区域建议、连通性可行性预测、或替代手写 flood-fill 启发（配合 [19-simd-flood.md](19-simd-flood.md)）。
- **RL（PPO）**：DFS 是**序列决策**（选变量 → 选值 → 观察回溯），收益延迟（分支好坏要等很久才知道）。监督学习（[06](06-ml-ordering.md)）用「子树大小」作标签是一种近似；RL 直接优化「求解步数/是否解出」，更适配长程信用分配。
- **Transformer**：把「题面 → 求解动作序列」当 seq2seq；或用其 attention 捕获格间长程依赖（compass 的半平面计数、rose 的符号分布是典型的**长程**约束，远超局部邻域）。
- 2488 题 + 每题 attempts 追踪是现成监督/交互信号源。

## 3. 现状与代码位置
- 无 ML 代码（`rsolver` 是纯 Rust 求解器；Python 层只有 IO/验证）。
- 监督信号：`results/bench/*.jsonl` 的 `attempts`（[05](05-ml-routing.md) 已规划使用）。
- 图结构：`rsolver/src/types.rs:123` `Puzzle`（cells/h_edges/v_edges/vertices）→ 可直接构图。

## 4. 收益
- 潜在把手写启发式升级为「问题自适应」；对档③（范式错配）、档⑤（根本难）硬题有突破潜力。
- GNN 的连通性预测若准确，可直接生成候选区域 → 大幅缩小搜索。

## 5. 代价与风险
- **风险：高**。需轨迹采集管线、训练基础设施、防过拟合（2488 题对深度学习仍算小数据）、模型嵌入 Rust 的部署复杂度；且收益**不确定**（可能不如简单启发式）。
- **代价**：大（数千行 + ML 栈 + 实验周期）。

## 6. 优先级 / ROI
- **P3 / 探索性**，ROI 低–中（不确定；24 §11.13 N12）。建议先做 [05](05-ml-routing.md)/[06](06-ml-ordering.md) 的轻量版验证「ML 在本题是否有信号」，再决定是否上重模型。

## 7. 实现思路
```
# 阶段 0（前置）：轨迹采集 —— 在分支点记录 (graph_snapshot, action, outcome)
# 阶段 1：GNN（PyG/DGL）
#   节点=格（特征：线索类型/数值/blocked/边界），边=邻接（特征：is_boundary/pre-boundary）
#   任务：链路预测（两格是否同区域）/ 节点分类（形状类别）
#   输出：候选区域建议 → 喂给搜索作优先分支
# 阶段 2：RL（PPO）
#   状态=搜索状态图，动作=选变量/选值，奖励=-搜索步数（解出给大正奖励）
#   环境=Rust 求解器暴露 step() 接口（或先纯 Python 原型）
# 阶段 3：Transformer seq2seq
#   输入=题面序列化，输出=模块执行序列 / 区域分配序列
```
- 部署：训练在 Python，推理导出为小模型/查表嵌入 Rust（或先用 Python 侧调度，避免改 Rust）。

## 8. 验证方法
- 离线：留出集上「GNN 预测连通性」的准确率；RL 的求解步数对比基线。
- 在线：`--baseline` REGRESSION=0；关注档③/⑤ NEW 解出。
- **先做信号验证**：用轻量模型（[05](05-ml-routing.md)）测「题面特征能否预测最优模块」—— 若连这个都学不到，重模型大概率无效。

## 9. 依赖与前置
- 前置（强）：[05-ml-routing.md](05-ml-routing.md)（先验证 ML 信号是否存在）、[06-ml-ordering.md](06-ml-ordering.md)、[14-meta-determinism.md](14-meta-determinism.md)。
- 评估依赖：[15-meta-evalproto.md](15-meta-evalproto.md)。

## 10. 参考
- `docs/优化/24` §11.13；`11` §4.5；GNN/RL for combinatorial optimization 文献。
