# 10 · 区域位向量 / SIMD

> 状态：🟢 新方向 ｜ 分类：底层系统调优 ｜ 来源：`docs/优化/24` §6.2
> 关联：[09-low-hashset.md](09-low-hashset.md) · [19-simd-flood.md](19-simd-flood.md)

## 1. 一句话
`cell_to_region` 用 `Vec<Option<usize>>`（8B/格）；连通性检查频繁整盘扫描。改用 `u16`/`u8` region id + 边界状态 `u64` 位向量（`popcount`/`ctz`），并探索整盘 SIMD 连通传播，降内存 + 提 cache 命中 + 加速顶点度检查。

## 2. 思想（为什么有效）
- 区域数 ≤ 256（实际远小于此）→ region id 用 `u8` 足够，`cell_to_region` 内存从 8B→1B/格（×8 压缩），cache 行装 8× 格，flood-fill / 边界判定 cache 命中率↑。
- 边界状态（`edge_csp` 的 `Vec<EdgeState>` 逐边分支）改用 `u64` 位向量：每条边 1 bit（cut/uncut），顶点度 = 相邻 4 bit 的 `popcount`，替代分支链。aog 的 `u32` bitfield 网格已是最佳实践（`aog/types.rs`），可把 `validate::validate` 的 `is_connected`（`validate.rs:393` 每区域 `HashSet`）改 DSU/位向量。
- 更深：整盘 cell 状态打包 `u64`×N（16×16=256 格 → 4×u64），一条 SIMD 指令推进多格连通传播（见 [19-simd-flood.md](19-simd-flood.md)）。

## 3. 现状与代码位置
- `backtrack.rs:108` `cell_to_region: Vec<Option<usize>>`。
- `edge_csp/mod.rs:42` `edges: Vec<EdgeState>`。
- `validate.rs:393` `is_connected`（HashSet）。
- aog `aog/types.rs` `u32` bitfield（参考）。

## 4. 收益
- 内存 ↓、cache ↑ → 所有整盘扫描加速（flood-fill、边界度、validate）。
- `is_connected` 从 O(HW) HashSet → O(区域数) DSU/位向量。

## 5. 代价与风险
- **风险：中**。位运算边界需仔细（off-by-one、顶点度语义）；`Option<usize>` 的 `None`（未决/blocked）需单独 bit 表示。
- **代价**：中（~200–400 行，改多处数据结构）。

## 6. 优先级 / ROI
- **P2**，ROI 中（广谱常数因子提升，但需改动多结构）。

## 7. 实现思路
```
// 区域 id 压缩
type Rid = u8;                       // region 数 ≤ 256
// 边界位向量
struct EdgeBits { cut: u64, uncut: u64 }   // 每 bit 一条边（按 (r,c,dir) 编码）
fn vertex_degree(v) -> u32 { (cut >> v*4 & 0xF).count_ones() }
// is_connected：DSU over cell→rid（或位向量 flood）
```
- SIMD 连通：用 `std::simd`（nightly）或 `packed_simd` 或手写 `u64` 移位，批量算邻接。

## 8. 验证方法
- 单元：随机盘，位向量结果与旧 `HashSet` 版一致。
- 全量 `--baseline` REGRESSION=0；关注 wall 下降。

## 9. 依赖与前置
- 与 [09-low-hashset.md](09-low-hashset.md) 协同（域位图即此处 `u64`）。

## 10. 参考
- `docs/优化/24` §6.2；`aog/types.rs`（u32 bitfield 参考）；`validate.rs:393`。
