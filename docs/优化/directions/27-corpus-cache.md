# 27 · 语料记忆化 / 解缓存

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N3）｜ 来源：`docs/优化/24` §11.12
> 关联：[17-fast-codec.md](17-fast-codec.md) · [26-puzzle-jit.md](26-puzzle-jit.md) · [15-meta-evalproto.md](15-meta-evalproto.md)

## 1. 一句话
① aog 形状库（`shapes.rs`）每题重建 → 跨 2488 题**共享一个持久化形状数据库**（mmap）；② **解缓存**：puzzle 内容 hash → 直接返回已知解（CI/全量回归重复跑同题零成本）；③ polyomino/dihedral 表一次性生成共享。

## 2. 思想（为什么有效）
- **形状库重建浪费**：aog 的 `core.shapes` + `shape_digest_index` + `node_to_shape_index`（`core.rs:27-28`）在**每道题**的求解开始时从头构建，但自由多格骨牌（free polyomino）及其 8 个 dihedral 变换是**与题无关**的通用结构 —— 完全可算一次、持久共享。
- **解缓存**：全量回归每天跑同一批 2488 题；`--baseline` 对比、`--retry-timeouts` 重跑会**重复求解同一题**。若 puzzle 的规范 hash 已有解记录，直接返回（含"已证无解/超时"的结论），批量墙钟骤降。
- **dihedral 键重算**：`shapes.rs:32` `dihedral_key` 每次调用算 8 个变换（`16` 指出这是热点，且 `validate.rs` 已有 `rid_to_key` 缓存但仅进程内）—— 持久化表可跨进程复用。

## 3. 现状与代码位置
- aog 形状库：`rsolver/src/solver/aog/core.rs:27-28`（`shape_digest_index`/`node_to_shape_index`），`core.rs:169` `shapes_insert`（运行时枚举插入）。
- `shapes.rs:32` `dihedral_key`（无跨进程缓存）、`shapes.rs:12` `is_rectangle`。
- `validate.rs:487` `rid_to_key`（进程内缓存）。
- polyomino：`rsolver/src/polyomino.rs:6` `transforms`（8 变换，无缓存）。
- Python 侧无解缓存：`rust_solver.solve` 每次都真解。

## 4. 收益
- 批量全量回归（2488 题）中，未变更题目的求解**零成本** → 回归时间从数十分钟降到几分钟。
- aog 形状库构建成本摊零；dihedral 计算跨进程复用。
- 与 [17-fast-codec.md](17-fast-codec.md) 协同，IO + 计算双降。

## 5. 代价与风险
- **风险：低**。缓存键必须含**全部**影响解的信息（grid、rules 及参数、线索、pre-boundaries、blocked）—— 键设计错误会返回过期解（严重）。建议用 puzzle JSON 的规范序列化 + sha256。
- **代价**：小–中（~150–400 行：hash 计算 + 缓存存储（SQLite/文件）+ 失效策略 + 形状库序列化）。
- **注意**：`AGENTS.md` 规定结果归档在 `results/bin|bench|tmp`；缓存文件需另置（如 `.cache/` 并加入 `.gitignore`）。

## 6. 优先级 / ROI
- **P1**，ROI 高（速赢，零算法风险；24 N3）。

## 7. 实现思路
```
// 1. 规范 hash
fn puzzle_hash(p: &Puzzle) -> String { sha256(canonical_json(p)) }
// canonical_json：字段排序、去掉无关字段（name/path/注释）
// 2. 解缓存（Python 侧最简，或 Rust 侧）
//    cache/solutions/<hash>.json  =  {solved, regions, solver, elapsed_ms, solver_version}
//    命中且 solver_version 匹配 → 直接返回（跳过子进程）
// 3. 形状库持久化（Rust 侧）
//    cache/shapes/<max_area>.bin  = 预生成的自由多格骨牌 + 8 变换 + dihedral key
//    启动时 mmap 载入，避免运行时重复枚举
// 4. 失效：solver 语义变更（规则语义/剪枝改动）时整体失效 → 用 solver_version 字段
```
- 缓存命中统计写入输出，便于评估。

## 8. 验证方法
- 一致性：清空缓存跑全量 vs 带缓存跑全量，结果完全一致（REGRESSION=0、NEW=0）。
- 安全性：改动 puzzle 任一字节后 hash 必变（单测）。
- 观察批量 wall 下降幅度与命中率。

## 9. 依赖与前置
- 协同：[17-fast-codec.md](17-fast-codec.md)（IO 与缓存叠加）、[26-puzzle-jit.md](26-puzzle-jit.md)（同为"重复求解"场景优化，缓存更直接）。

## 10. 参考
- `docs/优化/24` §11.12；`16`（dihedral_key 热点、`rid_to_key` 缓存）；`aog/core.rs:27-28,169`；`AGENTS.md` 归档规则。
