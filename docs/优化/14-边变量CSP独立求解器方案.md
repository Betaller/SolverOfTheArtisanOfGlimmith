# 边变量 CSP 独立求解器方案

> 状态：**已实现第一迭代 + 第二迭代进行中**（2026-08-14）。
> 实现详解见 `docs/rust-solver/11-edge-csp求解器.md`。第一迭代覆盖 ring/brick/area/
> precise/range/inequality/difference（14 道新解出，已合入 main，PR #33）；第二迭代
> 新增 fence（4 道）+ compass 方向计数基础 + 叶节点内部验证。watchtower/differentiation
> /solitary/compass桥网关留迭代三。本文件保留为设计方案。
> 决策来源：`docs/优化/12-优化项价值评估与路线图修订.md` §6.5 架构警告 + 用户决策
> （2026-08-08：架构不同→创建独立求解器，而非嵌入 aog）。
> 关联：`11-求解器优化理论总纲.md` §2（范式理论）、`12-优化项价值评估` §3.2 #10（方案B 终极方案）、
> `10-专用求解器方案.md` §3.5（BoundaryCSP 早期设计，本文是其升级与具体化）。
> 调研日期：2026-08-08（3 个 agent 深入调研参考求解器架构、本项目接入约定、FAIL 题特征）。

---

## 0. 一句话结论

本项目 aog 是**格子变量范式**（区域放置 DFS，`sp[x][y]` 位域网格），参考求解器
`third_party/aog` 是**边变量范式**（CSP，`Vec<EdgeState>` 三态边 + 传播-探测-回溯）——
**架构根本不同**。在 aog 内嵌边变量传播存在"区域 ID ↔ 边状态"转换的一致性 bug 风险
（`12` §6.5）。因此新建一个**独立的边变量 CSP 求解器** `rsolver/src/solver/edge_csp/`，
从 `third_party/aog` 1:1 移植传播引擎，内部维护三态边数组（不改全局 `Edge`），
走混合前置+后置路由，覆盖 ring/brick/watchtower/compass/inequality/difference 密集题。
**预计解出 8-15 道**（206 FAIL 的 4-7%），是第二波优化（需三态 Edge 的 6 项）的统一载体。

---

## 1. 为什么是独立求解器而非嵌入 aog

### 1.1 架构根本不同

| 维度 | 本项目 aog（`rsolver/src/solver/aog/`） | 参考 Rust aog（`third_party/aog/`） |
|---|---|---|
| **范式** | 格子变量（区域放置 DFS） | 边变量（CSP） |
| **核心状态** | `sp[x][y]` 位域网格，区域 ID 放在格子上 | `edges: Vec<EdgeState>` 三态边数组 |
| **边状态** | 隐含在区域 ID（同区域=Uncut，不同=Cut） | 显式 `EdgeState::Unknown/Cut/Uncut` |
| **传播** | 无传播链；靠形状放置 + `empty_area_check` 8 项检查 | `propagate()` 不动点循环 + 10 个传播器 + probing |
| **来源** | C++ AoG_Solver 1:1 移植（~3600 行） | lifthrasiir 原生 Rust（~8000 行核心） |
| **优势规则** | 形状匹配（D4 规范化哈希）、自由生长 | ring/brick 顶点度、watchtower、compass、面积传播 |

### 1.2 嵌入 aog 的架构警告

`12` §6.5 明确警告：aog 按 `sp[x][y]` 区域 ID 工作，不显式跟踪边状态。在 aog 内嵌边变量传播需要：
"区域 ID → 推断边状态 → 传播 → 强制边 → 回写区域 ID"，**每轮传播的双向转换极易引入一致性 bug**。
参考求解器从设计之初就是边变量原生的，没有这个问题。

### 1.3 独立求解器的优势

- **架构干净**：边变量范式原生，传播器直接读写 `EdgeState`，无转换开销与 bug 风险。
- **零回归**：不动 aog（1040/1052 PASS 的主力求解器），不动全局 `Edge` 结构体。
- **一次性吸收 6 项技术**：三态 Edge / 顶点度传播 / probing / Tarjan 桥 / compass 放置枚举 /
  GF(2)——这些是 `12` §3.1 列出的"第二波"全部，共同依赖三态 Edge 基础设施。
- **与 aog 范式互补**：aog 擅长形状题，边变量 CSP 擅长边约束题，按规则签名分发，零重复劳动。

---

## 2. 范式适用边界（哪些题走新求解器）

### 2.1 边变量 CSP 有优势的规则

| 规则 | FAIL 数 | 优势来源 |
|---|---|---|
| **ring** | 45（15 OOM + 27 NOSOL + 3 timeout） | 顶点度∈{0,2} + GF(2) 奇偶 → 传播确定大量边状态 |
| **brick** | 23（12 OOM + 11 NOSOL） | 顶点度≤3 传播 |
| **compass** | 43（31 timeout） | 边界框 O(1) 推导 + 放置枚举取交集强制边 |
| **watchtower** | 35 | 顶点周围件数约束，局部枚举 + 传播 |
| **inequality/difference** | 25+26 | 面积差分约束（Bellman-Ford），边变量下面积传播天然 |
| **fence** | 43 | arm_count 部分检查（DSU 预合并已证伪，仅 arm 不变量可用） |

### 2.2 边变量 CSP 无优势的规则（留给现有求解器）

| 规则 | 留给 | 原因 |
|---|---|---|
| rose_window | rose 求解器 | 符号配对 + 候选 BFS + MRV，rose solver 已是优解 |
| shape_pool/puzzle_piece | aog / pieces(DLX) | dihedral 形状匹配，需完整区域形状 |
| same/different/mixed | aog | 相邻区域形状比较，区域密封后才能检查 |

### 2.3 核心目标题

- **77 道纯边约束 FAIL**（ring/brick/watchtower/inequality/difference/compass，无 shape/rose/fence）
  ——核心目标，边变量 CSP 范式有优势且 aog/pieces/rose 无增量价值。
- **扩展 148 道含至少一个边约束规则的 FAIL**（允许混合 shape 规则，shape 部分不传播，靠 validate 兜底）。
- **不覆盖 58 道纯形状 FAIL**——留给 aog/rose/pieces。

---

## 3. 移植范围（完整边变量 CSP，~8000 行）

> 用户决策：完整移植，非最小版。从 `third_party/aog` 1:1 移植边变量 CSP 核心，
> 不移植形状放置 DFS（本项目已有 aog + pieces 覆盖）。

### 3.1 模块清单与参考映射

新求解器位于 `rsolver/src/solver/edge_csp/`，模块结构镜像参考求解器：

| 新模块 | 参考 `third_party/aog/src/` | 行数 | 移植难度 | 关键内容 |
|---|---|---|---|---|
| `edge_csp/types.rs` | `types.rs` | 160 | 低 | EdgeState/CellClue/EdgeClue/GlobalRules |
| `edge_csp/grid.rs` | `grid.rs` | 478 | 低 | 边索引 `h_edge/v_edge`、vertex_edges、cell_edges |
| `edge_csp/uf.rs` | `uf.rs` | 73 | 低 | ParityUF（带奇偶的并查集） |
| `edge_csp/state.rs` | `solver/edge_state.rs` + `solver/mod.rs` | ~600 | 低 | Solver 结构、set_edge、snapshot/restore（trail-based）、probe、EdgeForcer |
| `edge_csp/propagate.rs` | `solver/propagation/mod.rs` | 373 | 低 | propagate() 不动点循环 + probe_one_round + probe_pair_round |
| `edge_csp/prop/bricky_loopy.rs` | 同名 | 230 | 低 | ring/brick 顶点度传播到不动点 |
| `edge_csp/prop/area.rs` | 同名 | 2363 | **高** | build_components(flood fill) + area_bounds + compass 放置枚举 + complement 可行性 |
| `edge_csp/prop/dual.rs` | 同名 | 524 | 中 | Tarjan 桥 + 对偶连通性 + check_complement_feasibility |
| `edge_csp/prop/compass.rs` | 同名 | 432 | 中 | compass 基础 + 桥/网关强制 |
| `edge_csp/prop/watchtower.rs` | 同名 | 1039 | **高** | watchtower 顶点配置枚举 + 奇偶 |
| `edge_csp/prop/rose.rs` | 同名 | 336 | 中 | rose 分离/相位3/奇偶（处理 rose 混合题） |
| `edge_csp/prop/shape.rs` | 同名 | 661 | 中 | mingle/gemini/delta/mismatch 形状约束 |
| `edge_csp/prop/palisade.rs` | 同名 | 206 | 低 | fence(palisade) 旋转枚举 |
| `edge_csp/prop/delta_gemini.rs` | 同名 | 221 | 低 | gemini-delta 顶点几何交互 |
| `edge_csp/prop/loop_closure.rs` | 同名 | 277 | 中 | cut 图环路闭合检测 |
| `edge_csp/search.rs` | `solver/edges.rs` | 509 | 低 | select_edge（多因子评分）+ backtrack_edges（DFS 主循环） |
| `edge_csp/extract.rs` | `solver/validation.rs:compute_pieces` | ~200 | 低 | 边状态→区域重建（输出 RegionInfo） |
| **合计** | | **~8000** | | |

### 3.2 不移植的模块（本项目已有覆盖）

| 参考 `third_party/aog/src/` | 行数 | 不移植原因 |
|---|---|---|
| `parser.rs` / `formatter.rs` | 1789 | 本项目用 JSON（`io.rs`），新求解器吃 `&Puzzle` |
| `solver/pieces.rs`（backtrack_pieces DLX） | 613 | 本项目已有 `solver/pieces.rs` + `dlx.rs` |
| `solver/match_solver.rs` / `match_coupled.rs` | 1046 | 形状放置分派，本项目 aog 已覆盖 |
| `solver/clue_placements.rs` | 743 | compass 放置枚举已在 `prop/area.rs` + `prop/compass.rs` 内 |
| `solver/pair.rs` | 614 | rose 单元对分支，rose solver 已覆盖 |
| `main.rs`（测试） | 443 | 本项目走 `rsolver/src/main.rs` + Python 测试 |

### 3.3 需要新写的适配层

| 新模块 | 职责 | 行数 |
|---|---|---|
| `edge_csp/adapter.rs` | `rsolver::Puzzle → edge_csp::Puzzle`：规则名映射（rsolver `Rule.ctype` 字符串 → aog 规则枚举）、线索映射（compass up/down/left/right ↔ N/S/E/W、fence_pattern ↔ palisade、EdgeConstraintType Heterogeneous=delta/Homogeneous=gemini/Inequality/Difference）、预切边初始化（`is_boundary=true`→Cut，外边界→Cut，其余→Unknown） | ~300 |
| `edge_csp/mod.rs` | 入口 `solve_edge_csp(puzzle, start, timeout_ms) -> Option<Vec<RegionInfo>>`：adapter→Solver::new→solve→extract_regions | ~100 |

### 3.4 工作量估计

- **纯 1:1 移植**（传播引擎 + DFS 主循环）：~8000 行，参考代码已验证可工作，主要是适配与调试，**3-5 人日**。
- **适配层**（协议转换 + deadline + 测试）：~400 行，**1-2 人日**。
- **合计 4-7 人日**（含全量回归基准）。

---

## 4. 三态 Edge：内部维护，不改全局

> 用户决策：内部维护三态边数组，不改全局 `Edge` 结构体。

### 4.1 方案

新求解器内部定义 `enum EdgeState { Unknown, Cut, Uncut }` + `Vec<EdgeState>`（与 `puzzle.h_edges`/`v_edges`
同构布局），从 `Puzzle` 的 `is_boundary` 初始化：
- `is_boundary = true`（预绘制 / constraint 边）→ `Cut`
- 外边界 → `Cut`（隐式，边界检查时处理）
- 其余 → `Unknown`

传播完成后 flood-fill `Uncut` 边连通分量 → `region_of: Vec<Option<usize>>` → `rose::build_regions`。

### 4.2 优势

- **零改动面**：全局 `Edge.is_boundary: bool` 的 52 处读取（11 文件）全不受影响。
- **零回归风险**：与 aog/pieces/backtrack 完全隔离。
- **参考实现一致**：`third_party/aog` 就是这种架构（`edges: Vec<EdgeState>` 独立于 puzzle）。

### 4.3 代价

- aog/pieces/backtrack 无法直接受益于边变量传播结果。
- 若后续要让 aog 受益（如 compass 边界框预推导结果注入 aog bit-grid），需在 `solve()` 中
  临时修改 puzzle 或 aog 的 bit-grid seed（第二阶段评估）。

### 4.4 参考实现

- `third_party/aog/src/solver/edge_state.rs`（263 行）：`EdgeForcer` 批量收集 `(EdgeId, EdgeState)` 再 apply，
  `set_edge` 检测矛盾，`Snapshot { edges, diffs, sames }` 只记三个 Vec 长度，`restore` trail-based O(改动数)。
- `probe(setup)` 封装 `snapshot → setup → propagate → restore`。

---

## 5. 路由链接入：混合前置 + 后置

> 用户决策：混合前置+后置路由。

### 5.1 修订后的路由链

```
solve(puzzle, timeout_ms):
  1. pre_search_topology_check                              (已有)
  2. if is_solitary_compass_capable && SOLITARY_GROWER:     (WIP，已证伪见 §7)
       solitary::solve_solitary → 成功返回 / 失败 skip_aog
  3. if is_edge_csp_preempt(puzzle):                        ← 新增前置
       edge_csp::solve_edge_csp(puzzle, &start, preempt_budget)
       成功则返回；失败 fallthrough（不 skip_aog，让 aog 再试）
  4. if !skip_aog:
       aog::solve_aog(budget)                               (主力，1040/1052)
       if rose_capable: rose::solve_rose
  5. if is_edge_csp_capable(puzzle):                        ← 新增后置
       edge_csp::solve_edge_csp(puzzle, &start, fallback_budget)
  6. pieces::solve_pieces
  7. backtrack::solve_backtrack                             (0/1052 触发)
```

### 5.2 触发条件判断函数

参考 `is_rose_capable`（`mod.rs:294`）/ `is_solitary_compass_capable`（`mod.rs:308`）的写法：

```rust
// 后置 fallback：边约束密集题（ring/brick/watchtower/compass/inequality/difference）
fn is_edge_csp_capable(puzzle: &Puzzle) -> bool {
    let edge_rules = ["ring", "brick", "watchtower", "compass", "inequality", "difference"];
    puzzle.rules.iter().any(|r| edge_rules.contains(&r.ctype.as_str()))
}

// 前置拦截：aog 会 hang/OOM 的题（ring 无尺寸约束 → OOM 风险）
fn is_edge_csp_preempt(puzzle: &Puzzle) -> bool {
    let has_ring = puzzle.rules.iter().any(|r| r.ctype == "ring");
    let has_size = puzzle.rules.iter().any(|r| {
        matches!(r.ctype.as_str(), "precise" | "range" | "area" | "shape_pool" | "puzzle_piece")
    });
    has_ring && !has_size  // 21 OOM 中 13 道 ring 无尺寸约束
}
```

### 5.3 预算分配

参考现有单位预算制（`mod.rs:51-53`：每个求解器各自完整 `timeout_ms`）：
- **前置**：`preempt_budget = timeout_ms.saturating_sub(start.elapsed())`（像 solitary/rose 的剩余预算模式）。
- **后置**：`fallback_budget = timeout_ms`（像 pieces/backtrack 内部 `Instant::now() + Duration` 重起算）。

### 5.4 接入契约（必须满足）

1. 入口签名：`pub fn solve_edge_csp(puzzle: &Puzzle, start: &Instant, timeout_ms: u64) -> Option<Vec<RegionInfo>>`。
2. 输出 `Vec<RegionInfo>` 必须过 `validate::validate(puzzle, &regions)`（由 `build_solution` 自动执行）。
3. 用 `build_solution(regions, &start, puzzle, "edge_csp")` 包装（**非 trusted**，走全量验证）。
4. `mod.rs` 加 `pub mod edge_csp;` + 路由分派点（参考 solitary `mod.rs:84-103` 的接入模式）。

### 5.5 可复用的现有工具（无需重写）

- `rose::cells::{CellSet, PreBoundaries, edge_key}` — 边集合/位集/边键（`rose/cells.rs`）
- `rose::build_regions(region_of, h, w)` — cell→region 数组转 RegionInfo（`rose/mod.rs:45`，**边状态重建区域的出口**）
- `shapes::{dihedral_key, area_bounds, is_rectangle, collect_pool_shapes, rose_symbol_types}` — 形状/面积工具（`shapes.rs`）
- `validate::fence_pattern_shape`（`pub(crate)`）— fence 十字构造（中间剪枝必须用，保证与叶子验证器字节一致）
- `validate::validate` — 最终验收（`validate.rs`）

---

## 6. 核心算法骨架（从参考求解器移植）

### 6.1 传播引擎不动点循环（`propagate.rs`）

```
propagate() -> Result<bool, ()>:
  loop:
    progress = false
    run_prop!("bricky_loopy",  bricky||loopy,        propagate_bricky_loopy())    // 顶点度
    run_prop!("vertex_parity", !vertex_clues空,      propagate_vertex_edge_parity())
    run_prop!("loop_closure",  !probing && rose&&piece>=2, propagate_loop_closure())
    run_prop!("delta_gemini",  !edge_clues空,        propagate_delta_gemini())
    run_prop!("area_bounds",   true,                 propagate_area_bounds())     // 核心：build_components
    run_prop!("dual_conn",     rose && piece>=2,     propagate_dual_connectivity()) // Tarjan 桥
    run_prop!("rose_parity",   true,                 propagate_parity())
    run_prop!("rose_sep",      true,                 propagate_rose_separation())
    run_prop!("rose_phase3",   true,                 propagate_rose_phase3())
    run_prop!("palisade",      has_palisade,         propagate_palisade())
    run_prop!("compass",       has_compass,          propagate_compass())
    run_prop!("watchtower",    true,                 propagate_watchtower())
    if !progress:
      if !in_probing && 0<curr_unknown<=256: probe_one_round()   // 单边 probing
      if !progress: pair probing (curr_unknown<=10/20)
      if !progress: return Ok(true)
```

`area_bounds` 是**枢纽**：调 `build_components`（flood fill Uncut 连通组件）重建组件信息
（`curr_comp_id/sz`、`growth_edges`、`comp_cells`），所有其他传播器依赖这些缓存。

### 6.2 关键传播器

- **bricky_loopy**（230 行）：loopy 禁 3cut(T)，2cut+1unk→强制 Uncut，3cut+1unk→强制 Cut(成十字)；
  bricky 禁 4cut(+)，cut+unk>3→强制多余 unk 为 Uncut。
- **area**（2363 行，最复杂）：`build_components`、`growth_potential`（BFS 上界）、面积上下界强制、
  `propagate_compass_placement_enumeration`（max_area≤12 时穷举有效放置取交集强制边）、
  `check_complement_feasibility`（sealed 移除后剩余口袋必须能放下合法件）。
- **dual**（524 行）：`exact_piece_count>=2` 时，单生长边强制 Uncut；CC 数 vs 件数检查；
  Tarjan 桥（小侧<min_area→Uncut，大侧>max_area→Uncut）。
- **watchtower**（1039 行）：顶点周围 2×2 件数约束，`probe_watchtower_vertex_configs` 枚举配置。

### 6.3 DFS 主循环（`search.rs`）

```
backtrack_edges():
  if solution_count >= 2: return
  if curr_unknown == 0: pieces = compute_pieces(); if validate: save_solution; return
  (e, score) = select_edge()    // 多因子评分（见下）
  cut_first = prefer_cut_first(e)
  for val in [Cut, Uncut] or [Uncut, Cut]:
    snap = snapshot(); set_edge(e, val)
    match propagate(): Ok => backtrack_edges(), Err => {}
    restore(snap)
```

**select_edge 多因子评分**（`edges.rs:46`）：组件目标面积(+100)、密封有 target 组件(+75)、
线索约束组件(+30)、watchtower 顶点(+25)、rose 邻近(+40~200)、compass 邻近(+30~60)、
size_separation 冲突(+200 break)；`score>=200` 立即 break。

### 6.4 deadline 检查（必须新增）

参考求解器无硬超时。新求解器必须加：
- 传播阶段：每轮不动点迭代后检查 `Instant::now() >= deadline`。
- DFS 热循环：`steps % 1024 == 0 && Instant::now() >= deadline`（参考 backtrack `backtrack.rs:306`）。

### 6.5 边状态→区域重建（`extract.rs`，新写）

```
// EdgeState 传播完成后（curr_unknown==0）
// DSU 合并所有 Uncut 内部边两端 cell
// 每个 DSU 分量 = 一个 region，分配 region_id
// 收集 cells → Vec<[usize;2]>
// region_of: Vec<Option<usize>>  (row-major r*w+c)
// 调 rose::build_regions(region_of, h, w) → Vec<RegionInfo>
```

---

## 7. SolitaryGrower 方向证伪（同步最新结论）

> 注意：`12` §3.2 #1 把 SolitaryGrower 列为第一波 #1（预计 3-8 道）。此结论已被证伪。

**证伪依据**（记忆 `solitary-grower-disproven`，2026-08-08）：
- aog 的 `area_contain_symbol`（`aog/core.rs:210-214`）包含 `AREA_COMPASS_ENABLE`，
  `one_symbol_per_region` 分支（`aog/search.rs:726-764`）拒绝给区域加第二个 clue 格（含 compass）。
  **aog 已强制 solitary**。
- aog 解出 **9/24** 官方 compass+solitary 题（1ms-38s）。15 道超时的根因是 **aog 形状库内存爆炸**
  （开放网格 + compass 允许超大区域，如 0312 官方解有 28 格区域，形状库枚举 1-28 自由 polyomino 爆炸），
  **非缺 solitary 检查**。
- SolitaryGrower 实测：30s 预算跑 0312，8.9M 次 grow 调用未收敛，15 道全 FAIL 0 解。
  根因：开放网格无预画边界 → 每格可达每锚点 → 无强制传播 → MRV 每格 K 路分支（0312 = K^44）。

**对第一波路线图的修订**：
- `12` 第一波 #1 SolitaryGrower → **证伪，不做**。残留 `solitary.rs` 在 `solitary-grower` 分支，
  门控 `SOLITARY_GROWER=1` 默认关，未合 main。
- **正确方向**：15 道超时的根因是 aog 形状库无界增长 → **P0 shape cap**（`12` #3 aog 形状库硬上限）
  才是第一梯队最高杠杆项。第一波修订为：
  1. **aog 形状库硬上限**（~40 行，21 OOM，5-12 道）——原 #3 升为 #1
  2. **compass 边界框预推导**（~60-100 行，compass 43 FAIL，3-8 道）——原 #2
  3. **rose region_match visited 上限**（~25 行，#1 配套）

**对边变量 CSP 的影响**：
- compass+solitary 15 道不再需要 SolitaryGrower 拦截——由 aog shape cap 解决内存爆炸后 aog 自己能解
  （或解不出则后置 edge_csp 的 compass 放置枚举兜底）。
- 边变量 CSP 的 compass 放置枚举（`prop/area.rs` + `prop/compass.rs`）覆盖 compass-only 28 道
  （扣除 solitary 15 道后的 compass FAIL）。
- 路由链中 SolitaryGrower 前置分支（§5.1 步骤 2）可移除或保留门控默认关。

---

## 8. 预计收益与风险

### 8.1 收益估计

| 机制 | 预计解出 | 目标题 |
|---|---|---|
| ring+brick 顶点度传播 + DSU 合并 | 5-10 | ring 45 + brick 23 |
| compass 边界框 + 放置枚举取交集 | 4-8（部分与边界框重叠） | compass 28（扣 solitary） |
| Tarjan 桥分析 | 1-3 | rose + 连通性题 |
| WatchtowerPreFilter | 1-3 | watchtower 35 |
| probing（failed literal detection） | 含在上述 | 硬实例 |
| **合计（去重后）** | **8-15** | 占 206 FAIL 的 4-7% |

加上第一波（aog shape cap 5-12 + compass 边界框 3-8），合计 **16-27 道**（8-13%）。

### 8.2 风险

| 风险 | 等级 | 缓解 |
|---|---|---|
| 移植引入 bug（协议适配错误） | 中 | `build_solution` 走 `validate::validate` 全量复查，错解逃不过 |
| 性能不达预期（传播开销 > 收益） | 中 | 先移植核心传播器 + DFS，基准验证后再补 watchtower/compass 枚举 |
| 全量回归（1040 aog PASS 题） | 低 | 新求解器独立模块，不动 aog；前置/后置门控精确，只触发边约束题 |
| 与 aog 重复劳动 | 低 | `is_edge_csp_capable` 只对边约束密集题触发；形状题走 aog |
| compass 方向映射错误（N/S/E/W ↔ up/down/left/right） | 中 | adapter 单元测试覆盖 |

### 8.3 架构级优势

- **一次性吸收第二波全部 6 项**（`12` §3.1：三态 Edge / 顶点度传播 / probing / Tarjan 桥 /
  compass 放置枚举 / GF(2)），不需要分 6 次改造 aog。
- **三态 Edge 基础设施内置**（内部 `Vec<EdgeState>`），不需要全局升级（52 处读取不动）。
- **未来可升级**：验证收益后，可将内部三态边升级为全局 `Edge.state`，让 aog/pieces/backtrack
  受益于边变量传播结果（aog 内部 bit-grid 不受影响，主要是 backtrack/pieces/fence 的 25 处热路径）。

---

## 9. 实施顺序（分阶段）

### 阶段 0：前置依赖（第一波，无三态 Edge）
1. aog 形状库硬上限（~40 行）——解决 21 OOM，5-12 道
2. compass 边界框预推导（~60-100 行，预搜索阶段注入所有求解器）——3-8 道
3. rose visited 上限（~25 行）——配套

### 阶段 1：边变量 CSP 核心（最小可运行）
4. `edge_csp/` 骨架：types/grid/uf/state/adapter/mod + extract
5. `propagate.rs` + `prop/bricky_loopy.rs` + `prop/area.rs`（build_components + area_bounds）
6. `search.rs`（select_edge + backtrack_edges）+ deadline
7. 路由接入（后置 fallback，`is_edge_csp_capable`）
8. 基准测试：验证 ring/brick NOSOL 题解出数

### 阶段 2：传播器补全
9. `prop/dual.rs`（Tarjan 桥）+ `prop/compass.rs`（放置枚举）
10. `prop/watchtower.rs` + `prop/loop_closure.rs` + `prop/delta_gemini.rs` + `prop/palisade.rs`
11. probing（`probe_one_round` + `probe_pair_round`）
12. 前置路由（`is_edge_csp_preempt`，拦 ring OOM 题）

### 阶段 3：验证与调优
13. 全量回归基准（1258 题，归档 bin + bench）
14. 调优传播器顺序、probing 门槛、select_edge 评分权重
15. 评估是否升级全局三态 Edge

---

## 10. 验证方法

- **单元测试**：`edge_csp/` 各模块加 `#[cfg(test)] mod tests`，构造最小 ring/brick/compass puzzle
  验证传播+求解（参考 `solitary.rs:505-574` 模式）。
- **端到端**：Python `tests/integration/test_solver_end_to_end.py`，构造 puzzle→solve→validate。
  新求解器通过 `solution.solver == "edge_csp"` 归因。
- **基准门禁**（CLAUDE.md）：`scripts/benchmark_rust_solver.py --timeout 40 -j 8 --out`，
  归档 `results/bin/rsolver-<commit>-linux-x86_64` + `results/bench/<日期>_<commit>_edge-csp.txt`。
  验证：0 回归（1040 aog PASS 不掉）+ 新增解出数。
- **逐题验证**：对 77 道纯边约束 FAIL 题单独跑 `benchmark_rust_solver.py --dir`，确认解出。

---

## 11. 与现有文档的关系

| 文档 | 关系 |
|---|---|
| `11-求解器优化理论总纲` §2 | 范式理论（CSP/DLX/SAT/ILP/CP 对比），本文是其"方案B 完整边变量 DFS"的具体化 |
| `12-优化项价值评估` §3.2 #10 | 方案B 终极方案，本文将其从"长期方向"升级为"已决策方案" |
| `10-专用求解器方案` §3.5 | BoundaryCSP 早期设计，本文是其升级（完整移植 vs 从零设计） |
| `13-官方语料二级结论` | ring+brick 度≤2、watchtower=4∩brick=0 等结论，本文传播器直接利用 |

**本文的独特价值**：
1. 把"架构不同→独立求解器"的判断（`12` §6.5）从警告转为**已决策方案**。
2. 给出完整移植范围（~8000 行，模块级映射）、内部三态 Edge 方案、混合路由方案。
3. 同步 SolitaryGrower 证伪（`12` 第一波 #1 修订）。
4. 一次性吸收第二波 6 项技术，避免分 6 次改造 aog 的架构风险。

---

> **后续处理**：本文为设计方案（用户决策：只补文档，实现另开）。
> 实现时按 §9 分阶段执行，每阶段遵循 CLAUDE.md 文档软门禁：
> 同步 `docs/rust-solver/` 对应篇 + `docs/official-puzzles-status.md`，
> 跑通 pytest / cargo test / benchmark_rust_solver.py，归档 artifacts。
> 阶段 0（第一波）优先：aog shape cap + compass 边界框 + rose visited 上限。
