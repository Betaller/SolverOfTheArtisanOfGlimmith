# edge_csp 边变量 CSP 求解器

> 状态：**已实现**（第一迭代，2026-08-14）。
> 对应设计：`docs/优化/14-边变量CSP独立求解器方案.md`。
> 源码：`rsolver/src/solver/edge_csp/`。
> 参考实现：`third_party/aog`（lifthrasiir 原生 Rust 边变量求解器，~8000 行）。

## 0. 一句话结论

`edge_csp` 是一个**独立于 aog 的边变量 CSP 求解器**，显式维护三态边数组
（`Unknown` / `Cut` / `Uncut`），跑不动点传播（顶点度 + 面积界 + 线索传播 +
failed-literal 探测）+ 边 DFS。它不碰全局 `Edge.is_boundary`（52 处读取零改动），
输出经路由器 `validate::validate` 全量复查后才接受。

**第一迭代覆盖规则**：`ring` / `brick` / `area`（数字）/ `precise` / `range` /
`inequality` / `difference`。**尚未覆盖**：`compass`（方向计数）/ `watchtower` /
`differentiation` / `fence` / `solitary`（第二迭代）。

---

## 1. 为什么是独立求解器（回顾）

aog 是**格子变量范式**（`sp[x][y]` 位域网格，区域 ID 放格子），参考 aog 是
**边变量范式**（`Vec<EdgeState>` 三态 + 传播-探测-回溯）。在 aog 内嵌边变量传播需要
"区域 ID ↔ 边状态"双向转换，一致性 bug 风险高（`docs/优化/12` §6.5）。独立求解器原生
边变量，零转换，且不动 aog/pieces/backtrack/rose 任一主力路径。详见设计文档 §1。

## 2. 模块结构

```
rsolver/src/solver/edge_csp/
├── mod.rs      — Solver 结构 + set_edge/snapshot/restore/probe + select_edge +
│                 backtrack_edges + extract_regions + solve_edge_csp 入口 +
│                 is_edge_csp_capable / is_edge_csp_preempt 路由谓词
├── types.rs    — EdgeState / CompassData / CellClue(Area,Compass) /
│                 EdgeClueKind(Inequality,Diff) / VertexClue / GlobalRules
├── grid.rs     — 边索引几何（h_edge/v_edge/edge_cells/cell_edges/vertex_cells/
│                 edge_between/edge_vertices），参考 grid.rs 1:1 移植
├── adapter.rs  — crate::types::Puzzle → edge_csp Input（规则/线索映射 + 预切边初始化）
└── prop.rs     — propagate() 不动点循环 + bricky_loopy + build_components +
                  area_bounds + inequality/diff 传播 + probe_one/pair_round
```

### 2.1 adapter 的映射约定（关键）

| 项目模型 | edge_csp | 说明 |
|---|---|---|
| `Rule.ctype` `"ring"` / `"brick"` | `GlobalRules.loopy` / `.bricky` | 顶点度约束 |
| `Rule.ctype` `"precise"` / `"range"` | `eff_min_area` / `eff_max_area` | 全局面积界 |
| `Cell.number` | `CellClue::Area` | 面积数字 |
| `Cell.compass` (`up/down/left/right`) | `CellClue::Compass` (`n/s/e/w`) | `up→n`、`down→s`、`left→w`、`right→e` |
| `Edge.constraint` `Inequality(value)` | `EdgeClueKind::Inequality{smaller_first}` | `smaller_first = value != Some(1)`（`value==1` ⇒ 首端点更大） |
| `Edge.constraint` `Difference(value)` | `EdgeClueKind::Diff{value}` | 面积差 |
| `Edge.is_boundary` | 预切边（`Cut`） | 约束边在 `io.rs` 已设 `is_boundary=true`，故自动 Cut |
| `Vertex.watchtower` | `VertexClue` | 仅用于 select_edge 评分（值本身迭代二用） |

`heterogeneous`/`homogeneous`（异生/双生，形状 delta/gemini）**不移植**——边已 Cut，
形状关系由路由器验证器兜底。`cell_exists = !blocked`（blocked 格当空区）。

## 3. 核心算法（从参考 1:1 移植，剥掉 tracing/rose/shape）

### 3.1 传播不动点循环（`prop.rs::propagate`）

```
loop:
  if deadline 到 → Err（超时）
  bricky_loopy（ring/brick 顶点度）
  area_bounds（build_components + 面积目标封边 + inequality/diff）
  if 无进展:
    probe_one_round（单边 failed-literal，unknown≤256 时）
    probe_pair_round（顶点共边对探测，unknown≤10/20 时）
    if 仍无进展 → Ok(true)（稳定）
```

### 3.2 build_components（面积枢纽）

flood-fill **已决 Uncut** 边 → 连通组件；为每组件算目标面积（Area 线索 + compass
面积界）、min/max 面积、生长边（Unknown 跨界边）；组件达 max_area 时强制生长边 Cut；
组件目标面积互斥时强制 Cut（`cannot_merge`）。

### 3.3 顶点度传播（`prop.rs::propagate_bricky_loopy`）

**⚠ 与参考实现的关键差异（正确性修复）**：参考 aog 的 `bricky_loopy` 只数**内部边**
（`vertex_edges` 在网格边界返回 None），且它**从不在叶节点验证 ring/brick**——所以参考
会产出带**边界 T 型**（内部边界撞外边框成 3 段）的错误解。本项目 `validate.rs` 的 ring
检查（`count_boundary_edges_at_vertex`）**把外边框和 blocked 格边也当边界**。因此本移植
改 `bricky_loopy` 按 `count_boundary_edges_at_vertex` 语义数度：fillable-fillable 边看
状态、fillable-非fillable 边=边界、非fillable-非fillable 边≠边界。若不修，0666 等 ring
题会先找到"9 单格"错误解被验证器拒绝（见 §6 陷阱）。

ring（禁 3 段）/ brick（禁 4 段）/ ring+brick（度≤2）三态分别处理。

### 3.4 搜索（`mod.rs::backtrack_edges`）

`select_edge`（目标面积 + 密封 + watchtower 顶点多因子评分）→ `prefer_cut_first` →
`set_edge` + `propagate` → 递归回溯。`curr_unknown==0` 时 flood-fill 非 Cut 边重建区域
→ `rose::build_regions`。deadline 每 1024 节点 + 每轮传播检查。

### 3.5 输出与验收

`solve_edge_csp` 返回 `Option<Vec<RegionInfo>>`，**先过 `validate::validate` 全量复查
才返回 Some**（否则 None → 路由器回退）。所以 edge_csp 只可能 false-negative，不可能
false-positive。

## 4. 路由接入（`solver/mod.rs`）

```
solve():
  1. pre_search_topology_check
  2. aog（rose-capable 题给短预算）
  3. rose（rose-capable）
  4. is_edge_csp_capable → edge_csp::solve_edge_csp  ← 后置 fallback（新增）
  5. pieces（shape_pool/area/compass）
  6. backtrack
```

`is_edge_csp_capable`：**所有规则 ⊆ {ring,brick,watchtower,compass,inequality,difference,
area,precise,range} 且至少一条边规则**。排除 rose/shape/fence/boxy/solitary/differentiation/
heterogeneous/homogeneous——这些 edge_csp 不传播、只能靠叶节点验证器过滤，会烧光预算。

`is_edge_csp_preempt`（前置，ring 无尺寸约束 OOM 拦截）**已定义未接入**（迭代二）。

**Python 侧**：`src/solver/rust_solver.py` 的 `RUST_PARTS` 3→4（子进程墙钟预算
`timeout × RUST_PARTS × 1.2` 需覆盖 aog+edge_csp+pieces+backtrack 四个 unit 预算）。

## 5. 第一迭代实测

- `cargo test` 20 通过、`pytest` 290 通过、`cargo build --release` 仅历史遗留 warning。
- 新增求解（纯边约束 FAIL 题，aog 40s 超时后 edge_csp <13s 解出，均过验证）：
  0637 / 0638（difference+ring）、1134（inequality+ring）、0979（precise+ring）、
  1404（range+ring）、0507 / 0592 / 1400 / 0894 / 1382 / 1411（difference/inequality 系）。
  共 **~11 道**（与设计文档 §8 "8-15 道" 一致）。
- **0 回归**：edge_csp 独立模块 + 输出验证器兜底；后置 fallback 只在 aog/rose 失败后
  触发，各求解器仍是独立 unit 预算（无抢占）。

## 6. 实施陷阱（存档）

1. **bricky_loopy 外边框**（§3.3）：不数外边框 → ring 题产出边界 T 型错误解被验证器拒。
2. **路由 `return build_solution` 不 fall-through**：`build_solution` 验证失败返回
   `solved:false` 的 `Solution`（非 `Option`），直接 `return` 会吞掉 pieces/backtrack 的
   兜底。修正：`solve_edge_csp` 内部先 `validate`，失败返回 `None` 走回退（而非路由层 return）。
3. **`is_edge_csp_capable` 必须排他**：若对含 rose/shape 的题也触发，edge_csp 会在巨大
   搜索空间里找不存在的"满足未传播规则"的解，烧光预算。

## 7. 第二迭代（未做）

- `prop/compass.rs` + `propagate_compass_in_components`（方向计数，compass 43 FAIL）。
- `prop/watchtower.rs`（顶点配置枚举，watchtower 35 FAIL）。
- `propagate_size_separation`（differentiation）+ `propagate_boxy_nonboxy`（block/non_block）。
- `is_edge_csp_preempt` 前置接入（ring OOM 15 道，aog 现仍 exit -9 抢先）。
- 内部验证（叶节点 `validate` 通过才 save，无效继续搜）替代当前"首个解"入口验证，
  使 compass 题能越过方向计数错误的中间解。
