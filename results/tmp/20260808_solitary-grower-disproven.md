# SolitaryGrower 专用求解器 — 方向证伪归档

- **日期**：2026-08-08
- **分支**：`solitary-grower`（未提交，已回退工作区改动；`solitary.rs` 删除，`mod.rs` 恢复 main）
- **状态**：证伪，未合并，零行为变化
- **记忆**：[[solitary-grower-disproven]]

## 原始假设（错误）

> aog 的 `one_symbol_per_region` 只跟踪 `AREA_SYMBOL_BIT`（rose 符号），不跟踪 compass 格 → compass+solitary 题 aog 自由生长不强制 solitary → 15 道超时。SolitaryGrower（锚点 BFS + MRV + 增量 compass 剪枝）可解。

来源：Explore agent 的调研报告称 `symbol_loc` 只跟踪 `AREA_SYMBOL_BIT`。

## 证伪

`area_contain_symbol`（`rsolver/src/solver/aog/core.rs:210-214`）：

```rust
pub fn area_contain_symbol(&self, x: i32, y: i32) -> bool {
    let pv = self.puzzle[...][...];
    (pv & (AREA_PALISADE_INDEX_BIT | AREA_SLASH_INDEX_BIT | AREA_SHAPE_INDEX_BIT
        | AREA_SHAPE_SIZE_BIT | AREA_COMPASS_ENABLE | AREA_SYMBOL_BIT)) != 0
}
```

**包含 `AREA_COMPASS_ENABLE`**。`one_symbol_per_region` 分支（`aog/search.rs:726-764`）拒绝任何给区域加第二个 clue 格（含 compass）的扩张。**aog 已强制 solitary。**

铁证（基准 `results/bench/20260808_bd2f5f5_rose-pp-pin.jsonl`）：aog 解出 **9/24** 官方 compass+solitary 题：

```
Zone3/0034.json  compass+solitary            49ms
Zone3/0078.json  compass+solitary            51ms
Zone3/0083.json  compass+solitary            154ms
Zone3/0242.json  precise+compass+solitary    103ms
Zone3/0684.json  block+compass+solitary      1078ms
Zone3/0685.json  compass+solitary+homo+hetero 38441ms
Zone3/0698.json  compass+solitary            1ms
Zone3/0299.json  compass+solitary            1ms
Zone3/1016.json  compass+solitary            4246ms
Zone3/1391.json  compass+solitary            14ms
```

15 道超时**不是缺 solitary 检查**，而是 aog 形状库内存爆炸。

## 15 道 compass+solitary 超时清单（全 FAIL）

```
Zone3/0312.json  compass+solitary            timeout
Zone3/0680.json  compass+solitary            timeout
Zone3/0681.json  compass+solitary            timeout
Zone3/0682.json  compass+solitary            timeout
Zone3/0683.json  compass+solitary            timeout
Zone3/1017.json  compass+solitary            timeout
Zone3/1060.json  compass+solitary            timeout
Zone3/1079.json  compass+solitary            timeout
Zone3/1080.json  differentiation+compass+solitary  timeout
Zone3/1093.json  puzzle_piece+compass+solitary     timeout
Zone3/1246.json  compass+solitary            timeout
Zone3/1258.json  compass+solitary            timeout
Zone3/1259.json  compass+solitary            timeout
Zone3/1260.json  compass+solitary            timeout
Zone3/1109.json  block+area+compass+solitary timeout
```

## SolitaryGrower 实测

`rsolver/src/solver/solitary.rs`（已删）：锚点 = clue 格（K 区域）；同时生长 + MRV（选可达锚点最少的未分配格）；强制传播（单可达锚点）；增量 compass 上界剪枝；回溯。

- 30s 预算跑 0312：**8.9M 次 grow 调用**未收敛，deadline 在 30s 正常触发（不是死循环，是分支爆炸）。
- 15 道全 FAIL，0 解出。

根因：开放网格（0312 为 0 blocked / 0 boundary）→ 每格可达每锚点 → 无强制传播 → MRV 每格 K 路分支（0312 = 5 锚点，约 5^44）。比 aog 无结构优势。

## 0312 官方解结构（理解难度）

7×7，5 compass 格。官方解 5 区域大小 **8/28/8/4/1**：

```
compass (1,1): up=1  down=3  left=4  right=2   → 8 格区域
compass (1,5): up=6  down=19 left=18 right=5   → 28 格大区域
compass (3,3): up=1  down=3  left=2  right=4   → 8 格区域
compass (5,1): up=-1 down=-1 left=0  right=-1  → 4 格区域（strip 约束）
compass (5,5): up=-1 down=-1 left=-1 right=0   → 1 格区域（strip 约束）
```

区域大小跨度 1–28 → aog 枚举 1–28 格自由 polyomino，`shapes_insert`（`core.rs:165`，无 cap）爆炸。

## 结论与正确方向

15 道超时根因 = **aog 形状库无界增长** → P0-A1（shape cap + deadline 强制）。非专用求解器。

- **P0-A1**：aog `shapes_insert` 加硬上限（需处理 `all_shapes_different`/`homogeneous` 依赖 `shape_index` 的副作用，非平凡）。
- **P0-B**：rose `region_match` 的 `visited: HashSet<CellSet>`（`region_match.rs:40`）加上限，~30 行，最简单。
- 副产物：shape cap 后 aog deadline 能正常触发，compass+solitary 题不再挂死。

第一波（FenceSolver + SolitaryGrower + DiffAreaGraph）已两项证伪（FenceSolver 见 [[fence-pattern-dihedral-not-per-edge]]），DiffAreaGraph 待评。
