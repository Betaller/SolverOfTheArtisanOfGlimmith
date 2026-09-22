# edge_csp 边变量 CSP 求解器

> 状态：**已实现**（第一/二/三/四/五/六迭代已合入 main，PR #33/#37/#39/#41/#43/#45；第七迭代 differentiation + boxy/non_boxy 传播 2026-08-17）。
> 对应设计：`docs/优化/14-边变量CSP独立求解器方案.md`。
> 源码：`rsolver/src/solver/edge_csp/`。
> 参考实现：`third_party/aog`（lifthrasiir 原生 Rust 边变量求解器，~8000 行）。

## 0. 一句话结论

`edge_csp` 是一个**独立于 aog 的边变量 CSP 求解器**，显式维护三态边数组
（`Unknown` / `Cut` / `Uncut`），跑不动点传播（顶点度 + 面积界 + 线索传播 +
failed-literal 探测）+ 边 DFS。它不碰全局 `Edge.is_boundary`（52 处读取零改动），
输出经路由器 `validate::validate` 全量复查后才接受。

**第一迭代覆盖规则**：`ring` / `brick` / `area`（数字）/ `precise` / `range` /
`inequality` / `difference`（14 道新解出）。

**第二迭代新增**：`fence`（围栏/palisade 旋转枚举）+ `compass` 方向计数基础 +
**叶节点内部验证**（`validate` 通过才 save，继续搜）。fence 新增 4 道
（0923fix/0924fix/0903/0628）。

**尚未覆盖（迭代三）**：`watchtower` / `differentiation` / `solitary` / `block`
/`non_block`；compass 桥/网关强制（大 compass 题）；ring OOM 前置拦截。

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
| `Cell.fence_pattern`（3×3 十字） | `CellClue::Palisade{kind}` | `palisade_kind`：中心 `[1,1]` + 标记 `[0,1]`up/`[2,1]`down/`[1,0]`left/`[1,2]`right → `PalisadeKind`（None/One/Opposite/Adjacent/Three/All） |
| `Vertex.watchtower` | `VertexClue` | 仅用于 select_edge 评分（值本身迭代三用） |

`heterogeneous`/`homogeneous`（异生/双生，形状 delta/gemini）**不移植**——边已 Cut，
形状关系由路由器验证器兜底。`cell_exists = !blocked`（blocked 格当空区）。

## 3. 核心算法（从参考 1:1 移植，剥掉 tracing/rose/shape）

### 3.1 传播不动点循环（`prop.rs::propagate`）

```
loop:
  if deadline 到 → Err（超时）
  bricky_loopy（ring/brick 顶点度）
  vertex_edge_parity（watchtower 顶点奇偶）
  compass（值为 0 的方向强制 Cut）
  area_bounds（build_components + 面积目标封边 + inequality/diff/compass/solitary）
  if structural_pieces 有值 且 area_bounds 这轮没进展:
      dual_connectivity（D0/D1/D2，见 §14）
  watchtower / rose
  if 无进展:
    probe_one_round（单边 failed-literal，unknown≤256 时）
    probe_pair_round（顶点共边对探测，unknown≤10/20 时）
    if 仍无进展 → Ok(true)（稳定）
```

**为什么 `dual_connectivity` 要等 `area_bounds` 无进展**：`area_bounds` 内部
的传播器会写边，写完 `curr_comp_id` / `curr_comp_sz` 就是陈旧快照；D0/D2 读
这两个数组，拿陈旧 `num_comp` 算出的 `cc` 是假的（曾把 4 算成 10 并触发假
矛盾）。推迟到下一轮不动点，那时组件已重建。详见 §14 与
`docs/优化/30`。

### 3.2 build_components（面积枢纽）

flood-fill **已决 Uncut** 边 → 连通组件；为每组件算目标面积（Area 线索 + compass
面积界）、min/max 面积、生长边（Unknown 跨界边）；组件达 max_area 时强制生长边 Cut；
组件目标面积互斥时强制 Cut（`cannot_merge`）。

`build_components_growth_edges` 写边时会置 `Solver::build_progress = true`；
`propagate_area_bounds` 把它并进自己的返回值，否则不动点循环可能在边已变的
情况下提前收敛。

`propagate_area_constraints` 内部串行跑多个子传播器（areatgt → inequality →
diff → compass_in_comp → compass_enum → size_sep → boxy → solitary →
shape）。**任何一个报告 progress 就立刻 `build_components()` 重建组件**，再跑
下一个——否则后一个会拿陈旧连通性做推理（`solitary` S3 曾因此把已合并的组件
误判成"封闭无线索"）。

### 3.3 顶点度传播（`prop.rs::propagate_bricky_loopy`）

**⚠ 与参考实现的关键差异（正确性修复）**：参考 aog 的 `bricky_loopy` 只数**内部边**
（`vertex_edges` 在网格边界返回 None），且它**从不在叶节点验证 ring/brick**——所以参考
会产出带**边界 T 型**（内部边界撞外边框成 3 段）的错误解。本项目 `validate.rs` 的 ring
检查（`count_boundary_edges_at_vertex`）**把外边框和 blocked 格边也当边界**。因此本移植
改 `bricky_loopy` 按 `count_boundary_edges_at_vertex` 语义数度：fillable-fillable 边看
状态、fillable-非fillable 边=边界、非fillable-非fillable 边≠边界。若不修，0666 等 ring
题会先找到"9 单格"错误解被验证器拒绝（见 §6 陷阱）。

ring（禁 3 段）/ brick（禁 4 段）/ ring+brick（度≤2）三态分别处理。

**⚠ 赌博剪枝修复（2026-09-20，doc 29）**：超度时的 Uncut 强制原先实现为
「强制**前 n 条** Unknown Uncut」——只知道「至少 n 条须 Uncut」，挑哪几条是
赌博。1378 根层误强 (2,2) 的 W/E 后 palisade 判矛盾，官方解被剪掉（ring+brick
14 道 FAIL 的共同根因）。现仅在**全部** Unknown 都必须 Uncut 时才强制
（ring+brick `cut_count == 2`、bricky `cut_count == 3`）；`cut_count` 更低而
总数超限的情形只做矛盾检查，不挑边。新解 9 道（1373/1374b/1375/1378/0834/
0631/1110/0977/0978）。经验法则：传播器里「至少 n 选 k」只有 k==n（全选）或
k==0 才能落地为逐边强制，中间情形必须留给搜索。

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

## 7. 第二迭代（已实现：内部验证 + compass + fence）

- **内部叶节点验证**（P0）：`Solver` 存 `&'a Puzzle`，`backtrack_edges` 在
  `curr_unknown==0` 时 `extract_regions` 后先 `validate::validate` 通过才 save，
  否则继续回溯搜下一个（替代第一迭代的"首个解"入口验证）。是 compass/fence 的
  前置（它们的部分传播会产出中间无效解）。
- **compass 方向计数基础**（P1）：`propagate_compass`（0 方向邻边强制 Cut）+
  `propagate_compass_in_components`（组件方向计数 + 到限 Cut/缺限单网关 Uncut +
  两两相容 + 边界框剪生长边）。**大 compass 题仍超时**（需桥/网关强制，迭代三）。
- **fence 围栏**（P2）：`palisade_kind`（3×3 十字 → `PalisadeKind`）+
  `propagate_palisade_constraints`（4 旋转枚举取交集强制边）；`SUPPORTED` 加 `fence`。
  **新增 4 道**：0923fix / 0924fix / 0903 / 0628。

## 8. 第三迭代（已实现：compass 桥/网关；OOM 止血调研已回退）

- **compass 桥/网关强制**（P1，`docs/优化/20`）：`force_compass_via_bridges_and_gateways`
  （可达子图 + Tarjan 桥 + 单网关边强制 Uncut）+ `find_bridges_in_subgraph`（迭代 Tarjan）。
  新解出 0621（compass+difference ~3s）。
- **OOM 止血调研（已回退，净负）**（P0，`docs/优化/18`）：`DEFAULT_SHAPE_CAP` 0→50k 试验
  发现 16/21 OOM→优雅超时但**回归 ~12 道 aog 题**（其搜索合法超过 50k 库条目）→ 默认保持
  0（注释存档于 `aog/types.rs`）。`is_edge_csp_preempt` 细化但**不接入**——cap 开时冗余
  （后置 fallback 会接着跑）、cap 关时 moot，且只会把小块题重归因到 edge_csp 而无解出增益。
  **结论：OOM 止血需更精准手段（如 deadline 触发式 cap），50k 一刀切不可取。**

## 9. 第四迭代（已实现：watchtower 顶点传播）

- **watchtower 传播**（`prop.rs::propagate_watchtower`）：移植参考
  `third_party/aog/src/solver/propagation/watchtower.rs`，接入不动点循环（`!vertex_clues.is_empty()`
  门控，在 `area_bounds` 之后跑）。两遍：
  - **Pass A（component-ID）**：`curr_comp_id` 已填充时，数每个 watchtower 顶点
    周围 2×2 的 distinct sealed/growing 组件 → `[min_distinct, max_distinct]` 区间。
    `value` 越界→矛盾；`max_distinct==value && comp_count>1` 时强制不同组件间的
    Unknown 边 `Cut`。
  - **Pass B（edge-count）**：数 2×2 内部 4 边的 Cut/Unknown。cycle(4格)：
    `pieces=max(1,k)`；`value==1` 精确（强制 Uncut），`value≥2` 下界（强制 Cut 达 value）。
    tree(2-3格)：`pieces=1+k`；`value==2` 下界，`value==1/≥3` 精确。处理 double-touching
    （区域经两路径触顶点只算一次）。
- **value==1 启动优化**（`mod.rs::solve`）：内部顶点（4 格全在）`value==1` 预先强制
  4 条内部边 Uncut（Pass B 只在已有 cut 时行动，此优化主动解 `value==1⇒0 cut`）。
- `VertexClue.value` 不再 dead_code；`grid::vertex_pos` 逆映射 helper 新增。
- **新增 4 道** via edge_csp：0405 / 0419 / 0983 / 1140fix（均过验证 + 匹配官方解）。
  0 回归（1148 baseline-PASS 全量快扫 + 3 个"假回归"经隔离重跑确认是 aog 抖动/系统噪声，
  均无 watchtower、propagator 不触发）。

## 10. 第五迭代（已实现：watchtower parity 传播）

- **`propagate_vertex_edge_parity`**（`prop.rs`）：移植参考
  `watchtower.rs::propagate_vertex_edge_parity`（330-533），接入不动点循环
  （`!vertex_clues.is_empty()` 门控，在 `bricky_loopy` 之后）。新增独立
  `parity_uf.rs`（移植 `third_party/aog/src/uf.rs`，ParityUF XOR 并查集）。
  - 对每个 cut-count 奇偶**确定**的 watchtower 顶点，建未知内部边之间的成对 XOR
    约束，全局经并查集传播。奇偶只在以下确定：cycle(4格) 仅 value==4（k=4）；
    tree(2-3格) value∈{1,3,4}（k=value-1）。value≤3(cycle)/==2(tree) 因 double-
    touching 使 k 不定而跳过。
  - 三阶段：① 0/1/2 未知约束 → 校验/强制/union；② 3+ 未知约束用已在同 UF 分量的
    对约简；③ 已知边值经 UF 级联解剩余未知。
- **`probe_watchtower_vertex_configs`**（移植，但**禁用为 dead code**）：顶点配置
  枚举探测。实测在 85 题 watchtower 集上**较 parity 传播器 0 增量解**——parity UF
  已捕获可强制边。保留 `#[allow(dead_code)]` 供未来 compass+watchtower 题用。
- **新增 2 道** via edge_csp（较第四迭代）：0983 / 1000（均过验证 + 匹配官方）。
  0 真回归（2 假回归 0749/1270 无 watchtower + 隔离重跑仍解）。

## 11. 第六迭代（已实现：compass 放置枚举）

- **`propagate_compass_placement_enumeration`** + **`compass_placement_dfs`**
  （`prop.rs`）：移植参考 `area.rs::propagate_compass_placement_enumeration`
  （1742-2124）+ `compass_placement_dfs`（2129-2213），接入 `propagate_compass_in_components`
  之后。对每个 `max_area ≤ 12` 的 compass 线索：
  1. BFS 从 compass 格经非 Cut 边可达、限方向界 bbox 内的格。
  2. **全局 Uncut flood-fill** 建新鲜局部组件（关键：跨 bbox 跟 Uncut 边，已 committed
     到外部区域的格拖整片进来——防误强 Uncut）。
  3. DFS 枚举所有合法连通合并（include/exclude 分支，最小索引前沿优先），方向计数精确
     满足 + 尺寸界。cap：`MAX_AREA_THRESHOLD=12`/`MAX_REACHABLE_COMPS=16`/`MAX_PLACEMENTS=500`。
  4. `in_all`（每解都在）→ 强 Uncut；`in_any`（无解在）+ bbox 外 → 强 Cut。
  - 自门控 `in_probing`（昂贵，探测时跳过）；复用 `build_components` 的 `curr_min/max_area`。
- **新增 1 道** via edge_csp：0445（11×11 纯 compass 25 格，was OOM exit -9 → 73s 解出，
  过验证 + 匹配官方）。1140fix 也解出但是 iter4 已解的 flaky 临界题。
  0 真回归（1148 baseline-PASS 快扫 0 回归）。

## 12. 第七迭代（已实现：differentiation + boxy/non_boxy 传播）

- **`propagate_size_separation`**（`prop.rs`）：移植参考 `area.rs:370-485`。
  `differentiation`（相邻区域面积不同）：建 `sealed_neighbor_sizes[ci]`（相邻已 sealed
  或有固定 target 的组件的最终面积）→ Unknown 合并边若合并后面积等于某 sealed 邻居→强
  Cut；当前/target 面积被禁且仅剩 1 条 Unknown 生长边→强 Uncut。
- **`check_size_separation_sealed_pairs`**：两 sealed 等面积组件跨 Cut 边→矛盾（area.rs:1244-1265）。
- **`propagate_boxy_nonboxy`**（`prop.rs`）：移植参考 `area.rs:904-1001`。`block`（boxy：
  区域须矩形）/`non_block`（non_boxy：区域不可矩形）。矩形判定 = `cell_count == bbox_w*bbox_h`。
  sealed 非矩形(boxy)/矩形(non_boxy)→Err；growing 有填不满的洞(boxy)→Err；growing 恰 1 可填洞
  (non_boxy)→强 Cut 防矩形化。
- `GlobalRules` 加 `boxy`/`non_boxy` 字段；`is_edge_csp_capable` 的 SUPPORTED 加
  `differentiation`/`block`/`non_block`。
- **新增 3 道** via edge_csp（均 non_block+companion，过验证 + 匹配官方）：0690/0971/0984。
  differentiation 0 新解（propagator sound 但目标题未破）。boxy(block) 0 目标题（FAIL 集中
  block 题均带 rose/different/solitary，`is_edge_csp_capable` 仍排他）。
  0 真回归（2 假回归 0265/0875 经隔离重跑仍解）。

## 13. 第八迭代（未做）

- rose 范式迁移（`docs/优化/20` P2：pair.rs 对分支接进 edge_csp）。
  注：rose 伴生剪枝债 R1 已证伪（`docs/优化` 分支 `rose-companion-r1`）——rose 候选受
  ring 预切边限制 max≤8 无法覆盖 total，范式错配非剪枝可救，必须走 P2 边传播。
- pieces compass 枚举 `unwrap_or(0)` bug（`docs/优化/20`：spec<4 过度剪枝；分支
  `pieces-compass-fix` 已修但 0 新解，未合）。
- 纯 compass 0469/1395b 仍 FAIL（大单方向值 W=7/N=8/S=57 超 `MAX_AREA_THRESHOLD=12` 跳过，
  需桥/网关或调阈值；1395b 的 S=57 大列靠 bridge/gateway）。

## 14. 第九迭代（已实现：陈旧组件修复 + solitary 区域数锁定 + D0）

`docs/优化/30`。三处正确性修复、一处能力扩展、两个诊断开关。

### 14.1 陈旧组件快照（正确性）

`propagate_area_constraints` 的子传播器串行写边后不重建组件，下一个子传播器
读到过期的 `curr_comp_id` / `comp_cells` / `growth_edges`。典型后果：1017
官方解 Cut 播种后，`propagate_solitary` 的 S3 把 `(0,2..0,5)` 当成"封闭且
无线索"——实际上 `compass_enum` 刚把 `(0,1)-(0,2)` 强制 Uncut，它已经并进
有罗盘线索的区域 1 了。

修复：任何子传播器报 progress 就 `build_components()`；顶层
`propagate_dual_connectivity` 在 `area_bounds` 有进展时推迟到下一轮。

### 14.2 `exact_area` 不再把 `-1` 当 0（正确性）

`get_compass_area_bounds` 里 `exact_area = Some(1 + nv + sv)` 的 `nv` 来自
`n.unwrap_or(0)`。E/W 都是 0 时区域确实全在该列，但列长是 `n + s`；任一为
`-1` 就未知。现在四个方向都已知才算 exact。

### 14.3 `compass_enum` 门槛改用 bbox 面积（能力）

`curr_max_area` 是四个半平面之和，半平面互相重叠（东北格同时算 N 和 E），
系统性高估。1017 上它是 36，罗盘 bbox 只有 8–16 格，于是
`MAX_AREA_THRESHOLD = 12` 把全部线索跳过，`compass_enum` 在未播种棋盘上
根本不跑。改成 `max_a = min(curr_max_area, bbox 内可填充格数)`。

### 14.4 `solitary` → `structural_pieces` + D0（能力）

`structural_pieces` 第三个来源：`solitary` 下区域数 == 线索格数（
`validate::check_solitary` 同一谓词；87/87 官方解验证）。

新规则 **D0**（`propagate_dual_connectivity`）：组件只增不减，所以最终区域数
≤ 当前组件数。`num_comp < K` 矛盾；`num_comp == K` 时分区冻结——还需长大的
组件矛盾，所有跨组件 Unknown 边强制 Cut，然后直接返回（D1 会合并、D2 无边）。
D0 对 `precise` / `rose_window` 来源的件数同样生效。

### 14.5 诊断开关（新增，永久保留）

- `SKIP_AOG=1`：跳过 aog/rose，单独跑下游求解器（对偶 `AOG_ONLY`）。
- `EDGE_CSP_SKIP=bricky,compass,compass_in_comp,compass_enum,solitary,dual,probe,area,areatgt,areachk`：
  按名关传播器，不用重编即可二分 soundness bug。

### 14.6 实测

全量基准 1156 → **1157 / 1258**。`--rules solitary` 71 → 72/87。

- **0629**（compass+differentiation+ring）：aog 40s 超时后 edge_csp **2.3s**
  解出——bbox 面积门槛让 `compass_enum` 在这块棋盘上真正跑起来了。
- **0685** via aog。
- 1140fix 在 `-j 6` 下双 40s 超时翻负，串行复测两次均 61s SOLVED（争抢噪声）。
- 1017 官方解 Cut 播种：修复前根层矛盾，修复后 0 节点解出（D2 的
  `exact == cc` 直接把剩余 12 条全 Uncut）。未播种的 1017 仍超时（60 条内部
  边、要切约 26 条，D0/D2 的收益要等搜索把组件数压到 4）。

### 14.7 追加：S5d 潜在连通性 + 可行集搜索序

- `solitary_potential_connectivity`（S5d）：对非 Cut 边 flood-fill 得「仍可能连
  通」组件；可行集单点 `{i}` 的格子必须与线索 `i` 同属一个潜在组件，否则矛盾。
  整盘一次 flood-fill，O(格+边)。
- `select_edge` 在 `solitary_feasible_active` 时按端点可行集交集大小加分：交集
  越小越优先切，让 `cc` 尽快爬到 K，D2/D0 收尾。

`--rules solitary` 72 → 74/87；全量 1157 → **1159/1258**。**1017 与 1060 via
edge_csp 解出**；0685 并行翻负但串行 20s SOLVED（噪声）。

### 14.8 卫生：热路径 scratch 提升为字段

`build_components` 每次调用都 `vec![usize::MAX; n]` 分配 `id_map`，而它现在每个
不动点轮次要跑多次（每个有进展的子传播器一次）；`solitary_potential_connectivity`
的 `pot` 同理。两者都提成 `PropagationState::{id_map_buf, pot_buf}` 复用。

**踩坑（已记进代码注释）**：scratch 的清理必须放在函数**开头**而不是结尾。这两个
函数都有早退 `return Err(())` 路径，放结尾会在早退时被跳过，留下脏 scratch 给下
一次调用——`id_map` 脏了会让 `num_comp` 算错，1017 从 5s 解出变成 7ms 假穷尽。


## 2026-09-22 修订（area_sum 区域数源 + probe 超时护栏）

**`structural_pieces` 来源 4 — `area_sum_piece_count`**：面积线索的**去重值之和恰等于
可填格数**时，Σ k_v·v + Σ t_j = total 与 Σ v = total 迫使每个 k_v=1 且无无线索区，
区域数 = 去重值个数、尺寸多重集恰为那些值。官方语料中 number 恒伴随 area 规则
（218/218），推导健全；0262 {14,15,17,18}/64、1138 {60,61}/121、1183 {13,14,15}/42
三例答案均吻合。1183 由此解出。

**probe 超时护栏**：`probe_one_round`/`probe_pair_round` 原把「探针中途撞 deadline」
误读为失败文字而强制反值——逼近 deadline 时制造错误强制、污染搜索路径（0312 曾
3 次跑出 2 次的 flaky）。现探针超时直接收轮不强制；`probe_one_round` 保留「一轮
只强制一个字面量即返回」的原语义（实测 force-all 会因缺少强传播接力反而变弱：
0312 从 2/3 解出退化为 0/5）。修复后 0312 树完全确定（nodes 恒 44529）。
