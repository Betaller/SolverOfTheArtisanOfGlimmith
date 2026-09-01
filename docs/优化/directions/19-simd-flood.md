# 19 · 位并行 flood-fill

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N6）｜ 来源：`docs/优化/24` §11.4
> 关联：[10-low-bitvector.md](10-low-bitvector.md) · [04-parallel-gpu.md](04-parallel-gpu.md)

## 1. 一句话
把整盘 cell 状态打包成 `u64`×N 位向量（16×16=256 格 → 4×u64），**一条 SIMD/位运算指令同时推进多格连通传播**，重写 `empty_area_check`（aog 每放置一次全盘 flood-fill，O(cells²) 主热点）与 `is_connected`。

## 2. 思想（为什么有效）
- 连通性 flood-fill 是本题**唯一无法回避的高元约束**（`11` §2.1：连通性在所有通用范式中退化，手写 BFS 是最优常数因子）。既然躲不开，就把它做到极致。
- 位并行：用位向量表示「当前波前」，一次 `shift + OR + AND(邻居掩码)` 就是一轮 BFS 扩散——256 格的连通传播理论只需 O(格宽) 轮，每轮几条指令，而非逐格循环。
- `empty_area_check`（`search.rs:1265`，`aog/empty.rs`）是「每次形状放置都做一次全盘 flood-fill」，是 aog 最贵的单项（`11` §3/§6 反复提到）。位并行可把它降一个数量级。

## 3. 现状与代码位置
- aog `empty_area_check`：`rsolver/src/solver/aog/search.rs:1265` + `aog/empty.rs`（全盘 flood-fill）。
- `validate.rs:393` `is_connected`（每区域 HashSet）。
- aog 已有 `u32` bitfield 网格（`aog/types.rs`：LINE_BLOCK/AREA_BLOCK 等）——是位并行的良好基础。
- `edge_csp` 用 `CellSet` bitset（`edge_csp/mod.rs`），思路相近。

## 4. 收益
- aog 主热点（empty_area_check）常数因子数量级下降 → 直接提升所有走 aog 的题（实测 aog 首解占 97%，即绝大多数 PASS 题）的搜索速率。
- `is_connected` 从 HashSet → 位运算，validate 更快。

## 5. 代价与风险
- **风险：中**。位运算的边界（棋盘边缘、blocked 格、pre-boundary）极易 off-by-one；需详尽单元测。
- **代价**：中（~300–500 行：位向量布局 + 邻居掩码预计算 + 波前扩散循环 + 边界处理）。

## 6. 优先级 / ROI
- **P2**，ROI 高（aog 首解占 97%，其主热点加速惠及绝大多数题；但改动风险中，需详尽单元测；24 N6）。

## 7. 实现思路
```
// 布局：row-major，每行 ceil(W/64) 个 u64
struct GridBits { rows: Vec<u64>, stride: usize }
// 预计算邻居偏移掩码（上下左右，含边界屏蔽）
fn flood(wavefront: &mut GridBits, passable: &GridBits) {
    loop {
        let up    = shift_rows(&wavefront, -1) & !TOP_ROW_MASK;
        let down  = shift_rows(&wavefront, +1) & !BOTTOM_ROW_MASK;
        let left  = (wavefront >> 1) & !LEFT_COL_MASK;
        let right = (wavefront << 1) & !RIGHT_COL_MASK;
        let next = (up|down|left|right) & passable & !wavefront;
        if next.is_zero() { break; }
        *wavefront |= next;
    }
}
```
- 边界屏蔽掩码按 W/H 预计算一次（棋盘尺寸固定）。
- 可选 SIMD：`std::simd`（nightly）或 `packed_simd`；但纯 `u64` 位运算通常已足够（编译器可向量化）。

## 8. 验证方法
- 单元：随机盘 + 随机 blocked/boundary，位并行 flood 结果与朴素 BFS 完全一致（可达集相等）。
- `--baseline` REGRESSION=0 + 观察 aog 题 wall 下降。

## 9. 依赖与前置
- 前置：[10-low-bitvector.md](10-low-bitvector.md)（区域/边位向量基础）。
- 更远延伸：[04-parallel-gpu.md](04-parallel-gpu.md)（GPU 版位并行，低优先级）。

## 10. 参考
- `docs/优化/24` §11.4；`11` §2.1（连通性是核心难）、§3.4；`aog/types.rs`（u32 bitfield）。
