# 06 · 学变量序 / 值序

> 状态：🟢 新方向 ｜ 分类：机器学习 ｜ 来源：`docs/优化/24` §5.2
> 关联：[05-ml-routing.md](05-ml-routing.md) · [07-ml-nogood.md](07-ml-nogood.md) · [28-ml-deep.md](28-ml-deep.md)

## 1. 一句话
对 aog/backtrack 的「选哪格 / 选哪个形状 / 选哪个邻居区域」分支点，用历史求解轨迹学一个打分函数，把手写 MRV 链路升级为「问题自适应」排序，对档③/⑤ 硬题有突破潜力。

## 2. 思想（为什么有效）
- 标准 SAT/CP 的成功做法（VSIDS、冒泡排序学习）本质是「从历史冲突学变量序」。本题的 `pick_next_cell`（`backtrack.rs:955`）用手写 MRV + row-major tiebreak；aog 用 11 级优先级链。这些都是**静态**启发。
- 若从求解轨迹采样 `(state_features → chosen_branch → 是否导致回溯)`，可训练一个策略网络/梯度提升树，预测「哪个分支最可能导致快速求解 / 最少回溯」。这是把 SAT 的「学习」能力搬到手写 DFS。

## 3. 现状与代码位置
- 变量序：`backtrack.rs:955` `pick_next_cell`；`cell_domain_size`（`backtrack.rs:912`）。
- aog 优先级链：`aog/search.rs` 的 11 级 `mk_size` / `find_special_start_area`。
- 值序：分支点遍历 `valid_rids` + 新区域（`backtrack.rs:611-656`）。

## 4. 收益
- 可能把档③（范式错配）、档⑤（根本难）中「差一点」的题推过阈值。
- 比手写启发更适应题面分布（官方 2488 题覆盖广）。

## 5. 代价与风险
- **风险：中**。需插桩采集轨迹（轻微改动热路径）；模型过拟合到训练分布可能在某些题变差（需用 `--baseline` 广覆盖验证）。
- **代价**：中（轨迹采集 + 训练管线 + 模型嵌入；~300–500 行 + 离线训练脚本）。

## 6. 优先级 / ROI
- **P2**，ROI 中（潜力大但需实验验证；先 [05](05-ml-routing.md) 调度层见效后再做）。

## 7. 实现思路
1. **插桩**：在 `pick_next_cell` / aog 分支点记录 `(features, chosen, depth_gained_before_backtrack)`。
2. **特征**：frontier 大小、area 剩余、是否 clue 格、相邻区域形状多样性、顶点度紧张度…
3. **标签**：「该选择导致的子树大小 / 回溯次数」（越小越好）。
4. **训练**：梯度提升树（可解释）或 2 层 MLP。
5. **嵌入**：导出为查表/小网络；分支点用 `learned_score(branch)` 替/补手写序。
6. **安全网**：learned 序与 MRV 取 max 或加权融合，避免极端误排。

## 8. 验证方法
- 离线：轨迹上「learned 序 vs MRV」的子树大小对比。
- 在线：`--baseline` 要求 REGRESSION=0；重点看档③/⑤ NEW 解出。

## 9. 依赖与前置
- 前置：[05-ml-routing.md](05-ml-routing.md) 的训练基建可复用。
- 深化见 [28-ml-deep.md](28-ml-deep.md)（RL/Transformer）。

## 10. 参考
- `docs/优化/24` §5.2；`11` §3.1（MRV/LCV）；SAT VSIDS 文献。
