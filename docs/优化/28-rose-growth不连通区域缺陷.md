# 28 · rose_growth 产出不连通区域（25 道 FAIL 的共同上游）

> 状态：**已定位待修复**，2026-09-18。
> 定位：1131/1258 基准（`c58054d`）的 127 道 FAIL 中，**25 道**的 attempt 链
> 显示 `rose:validation_failed`——rose 求解器产出了通不过 `validate::validate`
> 的候选。本文记录根因定位，修复留待后续。

---

## 0. 一句话结论

rose 的两条路径里，`region_match`（精确覆盖，BFS 生成连通候选）产出的候选
是连通的；**不连通区域来自 `rose_growth` 兜底路径**——其 `swap_repair_iteration`
与 `repair_symbol_distribution` 在区域之间搬移格子时**不检查源区域是否仍连通**。

## 1. 定位过程

1. 在 1131 基准的 127 道 FAIL 里筛 `rose:validation_failed`：**25 道**
   （1098/1099/1433/0879/0390/0838/0660/0972/1156/1222/0987/0998/0839/0634/
   1403/0417/1402/0697/0776/1137/1392/1406/1150fix/1149a/1249）。
2. 给 `solver/validate.rs::validate` 的早返回点加临时打印，在 1433 上得到：
   **`validate FAIL: region 2 not connected (5 cells)`**。
3. 给 `rose/mod.rs` 两条 `accept_if_valid` 调用点加来源标记：1433 上
   **没有打印 region_match 的 REJECTED**，说明候选来自
   `rose_growth::solve_rose_growth` 兜底。
4. 读 `rose_growth.rs`：`wavefront_growth` 按邻接生长（连通性由构造保证），
   但后续两步会破坏它：
   - `swap_repair_iteration`（:285）→ `try_swap_fix` / `try_chain_move`：
     为修复**预划边界违规**而在区域间搬格，只查边界不查连通；
   - `repair_symbol_distribution`（:547）：为让每区恰含一个符号而搬格，
     同样不查连通。

## 2. 为什么值得修

25 道是当前最大的单一失败簇。它们的共同形态是：aog 拿满 20s 超时 →
rose 在 1~8s 内产出一个**结构非法**的候选被 `validate` 拒绝 → 路由器落到
edge_csp/pieces（多数 `not_attempted`，因为纯 rose 题不在 edge_csp 门内）。
若 rose 产出的候选保持连通，其中一部分会直接变成 PASS。

注意：`accept_if_valid` 的拒绝是**正确行为**（路由器的独立复验网关，
CLAUDE.md 关键不变量），本缺陷是"解不出"而非"错解被接受"。

## 3. 修复方向

1. **搬移后连通性检查**：`try_swap_fix` / `try_chain_move` /
   `repair_symbol_distribution` 每次把格 `c` 从区域 A 搬到区域 B 后，
   对 A 做一次 BFS/DFS 连通性校验（区域规模小，代价可接受）；不连通则回滚。
2. **只搬"叶子"格**：更便宜的近似——只允许搬移在源区域中度数为 1 的格
   （搬走后不会把区域切成两半）。可作快速路径，连通性检查兜底。
3. **搬移后统一校验**：两步 repair 结束后先自检连通性，不连通则整体放弃
   该候选（返回 None），让路由器走下一个求解器——比错解被接受好，但会
   丢掉"差一步合法"的候选，收益低于方案 1。

推荐方案 1（精确）+ 方案 2（快速路径）。

## 4. 关联

- `rsolver/src/solver/rose/rose_growth.rs`（swap_repair / symbol repair）
- `rsolver/src/solver/rose/mod.rs::accept_if_valid`
- `rsolver/src/solver/validate.rs::validate`（连通性早返回）
- 基准：`results/bench/20260918_c58054d_shape-identity-compass-dual.jsonl`
