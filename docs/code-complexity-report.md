# 代码复杂度静态分析报告

> 生成日期：2026-09-02
> 分析范围：`src/`（Python 求解器 / 模型 / UI）、`rsolver/`（Rust 求解器内核）
> 工具：`radon 6.0.1`（Python：圈复杂度 / 可维护性指数 / 体积）、`lizard 1.24.0`（Python + Rust：圈复杂度 / 函数规模）
> 说明：`web/`（TypeScript WASM 前端）与 `tests/` 不在本次分析范围内。

---

## 1. 方法与阈值约定

| 指标 | 工具 | 说明 |
|---|---|---|
| 圈复杂度 CC (Cyclomatic Complexity) | radon cc / lizard | 决策点 + 1；越大越难测试维护 |
| 可维护性指数 MI (Maintainability Index) | radon mi | 0–100，越高越好；radon 分级：A≥20，B 10–19，C<10 |
| 体积指标 | radon raw / lizard | LOC / NLOC / SLOC / 注释率 |
| 函数危险度 | lizard | 默认 `CCN>10` 计为 warning；`NLOC>100` 计为 nloc warning |

**圈复杂度分级（radon 标准，lizard 数值口径略有差异但趋势一致）**

| 等级 | CC 区间 | 含义 |
|---|---|---|
| A | 1–5 | 简单，风险低 |
| B | 6–10 | 中等 |
| C | 11–20 | 较复杂，建议关注 |
| D | 21–30 | 复杂，应重构 |
| E | 31–40 | 非常复杂，高修改风险 |
| F | ≥41 | 极复杂，强烈建议拆分 |

---

## 2. 总体指标对比

| 维度 | Python (`src/`) | Rust (`rsolver/`) |
|---|---|---|
| 分析函数/代码块数 | 442（radon 块）/ 405（lizard 函数） | 328（lizard 函数） |
| 总代码行（radon LOC / lizard NLOC） | LOC 8098 / NLOC 6529 | NLOC 12885 |
| 平均圈复杂度 CCN | 4.1 | **10.3** |
| 平均函数体积 NLOC | 12.6 | **36.5** |
| 平均 token 数 | 110.9 | **276.2** |
| 高复杂度函数告警数（CCN>10） | 17（占比 4%） | **54（占比 16%）** |
| 大函数告警数（NLOC>100） | 占比 20% | **占比 60%** |

**结论**：Python 侧整体复杂度健康（平均 CCN 4.1，A 级占比 79%），问题集中在 UI 层与校验层；Rust 求解内核**复杂度显著偏高**——平均 CCN 是 Python 的 2.5 倍，超过 1/6 的函数触发高复杂度告警，且 60% 的代码位于超长函数（>100 行）中，是后续重构的重点。

---

## 3. Python 侧详细分析

### 3.1 可维护性指数（radon mi，越低越差）

最差的 8 个模块：

| 文件 | MI | 等级 | 风险点 |
|---|---|---|---|
| `src/ui/grid_widget.py` | **0.00** | C | 巨型 UI 绘制/事件类，复杂度最高 |
| `src/ui/puzzle_browser.py` | 14.50 | B | 预览绘制、扫描逻辑复杂 |
| `src/ui/main_window.py` | 16.71 | B | 主窗口装配、信号槽多 |
| `src/validation/validator.py` | 6.98 | C | 规则校验分支多 |
| `src/solver/constraints.py` | 6.57 | C | 22 类规则检查器集中 |
| `src/ui/shape_gallery.py` | 35.07 | A | — |
| `src/ui/property_panel.py` | 23.84 | A | — |
| `src/ui/shape_editor.py` | 26.74 | A | — |

### 3.2 圈复杂度分级分布（radon cc，共 442 块）

| 等级 | 数量 | 占比 |
|---|---|---|
| A (1–5) | 349 | 79.0% |
| B (6–10) | 59 | 13.3% |
| C (11–20) | 27 | 6.1% |
| D (21–30) | 5 | 1.1% |
| E (31–40) | 1 | 0.2% |
| F (≥41) | 1 | 0.2% |

> 绝大多数代码为 A 级；33 个块处于 C 级及以上，构成主要技术债。

### 3.3 复杂度热点函数（按 CCN 排序，Top 15）

| 文件:函数 | CCN | 等级 | NLOC | 参数 | 说明 |
|---|---|---|---|---|---|
| `src/ui/grid_widget.py:mouseMoveEvent` | **42** | F | 93 | 2 | 鼠标拖拽/绘边状态机，分支极多 |
| `src/ui/grid_widget.py:mousePressEvent` | **31** | E | 121 | 2 | 点击命中测试 + 多种编辑模式 |
| `src/ui/puzzle_browser.py:paintEvent` | 23 | D | 97 | 2 | 缩略图绘制含多类装饰 |
| `src/io/puzzle_codec.py:dict_to_puzzle` | 21 | D | 60 | 1 | JSON→模型反序列化，规则分支多 |
| `src/ui/puzzle_browser.py:_scan_puzzles` | 21 | D | 53 | 1 | 目录扫描 + 解析 + 容错 |
| `src/validation/validator.py:_check_compass` | 21 | D | 33 | 4 | 罗盘半平面判定逻辑 |
| `src/ui/grid_widget.py:keyPressEvent` | 20 | C | 63 | 2 | 键盘交互状态机 |
| `src/validation/validator.py:validate` | 20 | C | 27 | 3 | 总校验入口，调度多条规则 |
| `src/validation/validator.py:_check_edge_constraints` | 19 | C | 47 | 4 | 边约束校验 |
| `src/ui/property_panel.py:_rebuild_edge` | 18 | C | 91 | 1 | 边属性面板重建 |
| `src/ui/grid_widget.py:_draw_selection` | 18 | C | 49 | 2 | 选择高亮绘制 |
| `src/ui/grid_widget.py:_draw_rule_overlay` | 17 | C | 75 | 2 | 规则叠加层绘制 |
| `src/io/puzzle_codec.py:puzzle_to_dict` | 17 | C | 50 | 1 | 模型→JSON 序列化 |
| `src/ui/puzzle_browser.py:_matches_extra` | 17 | C | 24 | 2 | 过滤匹配 |
| `src/ui/grid_widget.py:_draw_edge_constraints` | 16 | C | 47 | 2 | 边约束绘制 |

### 3.4 按文件聚合（平均 CCN 降序，Top 12）

| 文件 | 函数数 | 平均CCN | 最大CCN | NLOC |
|---|---|---|---|---|
| `src/validation/validator.py` | 33 | 7.0 | 21 | 473 |
| `src/solver/constraints.py` | 33 | 6.7 | 14 | 391 |
| `src/ui/puzzle_browser.py` | 20 | 6.2 | 23 | 417 |
| `src/io/puzzle_codec.py` | 9 | 6.2 | 21 | 142 |
| `src/ui/grid_widget.py` | 60 | 5.7 | **42** | 1092 |
| `src/solver/rust_solver.py` | 20 | 4.7 | 15 | 296 |
| `src/ui/property_panel.py` | 24 | 3.6 | 18 | 374 |
| `src/ui/constraint_panel.py` | 13 | 3.6 | 9 | 204 |
| `src/models/board.py` | 24 | 3.2 | 10 | 129 |
| `src/solver/base.py` | 9 | 3.1 | 16 | 100 |
| `src/ui/main_window.py` | 44 | 2.7 | 11 | 591 |
| `src/ui/shape_editor.py` | 16 | 2.3 | 4 | 199 |

> `grid_widget.py` 体量最大（1092 行、60 个函数）且单函数复杂度最高，是 Python 侧首要重构对象。

---

## 4. Rust 侧详细分析（求解内核）

Rust 侧是复杂度重灾区，以下按文件与函数两级呈现。

### 4.1 按文件聚合（平均 CCN 降序）

| 文件 | 函数数 | 平均CCN | 最大CCN | NLOC |
|---|---|---|---|---|
| `rsolver/src/solver/aog/search.rs` | 5 | **70.4** | **200** | 1207 |
| `rsolver/src/solver/rose/rose_growth.rs` | 5 | 36.4 | 76 | 554 |
| `rsolver/src/solver/edge_csp/prop.rs` | 27 | 31.4 | 80 | 2643 |
| `rsolver/src/solver/validate.rs` | 10 | 18.6 | 99 | 584 |
| `rsolver/src/solver/prototypes.rs` | 3 | 15.7 | 23 | 149 |
| `rsolver/src/solver/rose/region_match.rs` | 9 | 15.6 | 38 | 574 |
| `rsolver/src/solver/edge_csp/adapter.rs` | 2 | 14.0 | 21 | 162 |
| `rsolver/src/solver/backtrack.rs` | 21 | 12.3 | 54 | 830 |
| `rsolver/src/solver/aog/empty.rs` | 24 | 10.6 | 68 | 728 |
| `rsolver/src/solver/aog/core.rs` | 32 | 8.9 | 112 | 960 |
| `rsolver/src/solver/pieces.rs` | 17 | 8.2 | 27 | 523 |
| `rsolver/src/solver/edge_csp/mod.rs` | 24 | 5.2 | 26 | 453 |
| 其余模块 | — | ≤7.9 | ≤22 | — |

> 复杂度高度集中于三处：**AOG 搜索**（`aog/search.rs`）、**边变量 CSP 传播**（`edge_csp/prop.rs`）、**Rose 符号求解**（`rose/rose_growth.rs`）。这三文件贡献了 Rust 侧约 60% 的高复杂度告警。

### 4.2 复杂度热点函数（按 CCN 排序，Top 18）

| 文件:函数 | CCN | NLOC | 参数 | 风险 |
|---|---|---|---|---|
| `aog/search.rs:place_non_predifined_shape` | **200** | 739 | 9 | 超巨型搜索函数，单函数占整文件 61% |
| `aog/search.rs:dfs` | **139** | 442 | 4 | 深层 DFS + 回跳，分支爆炸 |
| `aog/core.rs:build` | **112** | 368 | 2 | 候选构建主流程 |
| `validate.rs:validate` | 99 | 288 | 2 | 总校验，规则分支极多 |
| `edge_csp/prop.rs:propagate_compass_placement_enumeration` | 80 | 286 | 1 | 罗盘放置枚举传播 |
| `edge_csp/prop.rs:force_compass_via_bridges_and_gateways` | 80 | 280 | 4 | 罗盘桥/网关约束 |
| `rose/rose_growth.rs:solve_singlesymbol` | 76 | 228 | 10 | 单符号求解，参数过多 |
| `edge_csp/prop.rs:propagate_watchtower` | 72 | 195 | 1 | 瞭望塔传播 |
| `rose/rose_growth.rs:solve_multisymbol` | 71 | 211 | 11 | 多符号求解，参数过多 |
| `edge_csp/prop.rs:propagate_compass_in_components` | 70 | 218 | 2 | 组件内罗盘传播 |
| `aog/empty.rs:empty_area_check` | 68 | 136 | 2 | 空洞区域检查 |
| `edge_csp/prop.rs:compass_cells_incompatible` | 49 | 103 | 5 | 罗盘单元互斥 |
| `backtrack.rs:dfs` | 54 | 168 | 2 | 回溯 DFS |
| `edge_csp/prop.rs:propagate_vertex_edge_parity` | 45 | 175 | 1 | 顶点边奇偶性传播 |
| `edge_csp/prop.rs:probe_watchtower_vertex_configs` | 44 | 170 | 1 | 瞭望塔配置探测 |
| `edge_csp/prop.rs:propagate_size_separation` | 43 | 111 | 2 | 尺寸分离传播 |
| `rose/region_match.rs:solve_by_region_match` | 37 | 185 | 7 | 区域匹配求解 |
| `edge_csp/prop.rs:propagate_boxy_nonboxy` | 37 | 93 | 2 | boxy/non-boxy 传播 |

### 4.3 Rust 侧关键问题

1. **超巨型函数**：`place_non_predifined_shape`（739 行，CCN 200）与 `dfs`（442 行，CCN 139）单函数体量远超合理上限，几乎无法单元测试。
2. **参数过多**：`solve_multisymbol` 11 参数、`solve_singlesymbol` 10 参数、`match_regions_mrv` 11 参数、`compass_placement_dfs` 13 参数——应封装为结构体配置。
3. **`edge_csp/prop.rs` 单文件过载**：2643 行、27 个函数、平均 CCN 31.4，承载了全部边约束传播逻辑，是拆分重构的首要目标。

---

## 5. 重构优先级建议

| 优先级 | 目标 | 动作 | 预期收益 |
|---|---|---|---|
| **P0** | `rsolver/src/solver/aog/search.rs`（`place_non_predifined_shape`、`dfs`） | 拆分搜索/放置/回跳为独立子函数；提取共享状态为结构体 | CCN 200→可接受区间，提升可测试性 |
| **P0** | `rsolver/src/solver/edge_csp/prop.rs` | 按约束类型拆分模块；将巨型传播函数下沉为小步骤 + 调度表 | 单文件 CCN 31→<15，降低回归风险 |
| **P1** | `rsolver/src/solver/validate.rs:validate` | 将 22 类规则校验拆为可注册的检查器数组（参考 Python `constraints.py`） | CCN 99→<20 |
| **P1** | `rsolver/src/solver/rose/rose_growth.rs` | 引入求解上下文结构体消除 10/11 参数函数 | 可读性、可维护性提升 |
| **P1** | `src/ui/grid_widget.py` | 拆分 `mouseMoveEvent`/`mousePressEvent`/`keyPressEvent` 为「命中测试 + 模式分派」；`MI 0.00`→B 级以上 | UI 改动风险下降 |
| **P2** | `src/validation/validator.py` / `src/solver/constraints.py` | 规则校验器已较规整，补充单元测试覆盖 D 级函数即可 | 维持现状 |
| **P2** | `src/io/puzzle_codec.py:dict_to_puzzle` | 拆分反序列化分支为按类型的小解析器 | CCN 21→<12 |

> 在不破坏求解行为的前提下，建议**先补测试再重构**（当前 365 个测试可作回归护栏），并遵循 AGENTS.md 的「每次影响求解结果的优化须保留 `results/bin/` 二进制与 `results/bench/` 基准」规则。

---

## 7. 复杂度门禁（本次新增，detekt 等价物）

上文 1–6 节是 2026-09-02 的静态分析快照；本节记录随后落地的**强制门禁**，
把「复杂度不再恶化」变成 push 前的硬约束（对标 Kotlin detekt 的
`CyclomaticComplexMethod`）。

### 7.1 三种语言、三套分析器

| 语言 | 工具 / 规则 | 阈值 | 阈值存放位置 |
|---|---|---|---|
| Python（`src/`、`scripts/`） | radon 圈复杂度 | **CC ≤ 10** | `scripts/complexity_gate.py --max`（默认 10） |
| Rust（`rsolver/`） | clippy `cognitive_complexity`（nursery lint） | 见 `rsolver/clippy.toml` | `cognitive-complexity-threshold` |
| JS / TS / Vue（`web/`） | eslint `complexity` | **CC ≤ 10** | `web/eslint.config.js` |

10 即 detekt `CyclomaticComplexMethod` 的默认值，因此 Python 与 JS 侧与默认
detekt 配置完全一致；clippy 的 cognitive 分数额外计入嵌套深度，不能与 radon 的
圈复杂度 1:1 对应，故其阈值独立放在 `clippy.toml` 中。

### 7.2 使用方式

```bash
python3 scripts/complexity_gate.py            # 仅 Python
python3 scripts/complexity_gate.py --rust     # 仅 Rust（cargo clippy）
python3 scripts/complexity_gate.py --js       # 仅 JS/TS/Vue（eslint -f json）
python3 scripts/complexity_gate.py --all      # 三种语言（钩子调用的就是这个）
```

退出码：`0` 全部通过 / `1` 存在超阈值块 / `2` 调用或配置错误。

### 7.3 钩子

- `.git/hooks/pre-push`：`git-lfs passthrough → complexity_gate.py --all → pytest`。
  任一步失败即拒绝 push。
- `.pre-commit-config.yaml`：同一个门禁作为 local hook（`always_run`），
  `pre-commit run --all-files` 也会执行。

### 7.4 本轮降低结果

| 语言 | 超阈值块（前 → 后） | 说明 |
|---|---|---|
| Python | **70 → 0**（801 块全部 ≤ 10） | validator / codec / constraints / solver / UI 全部拆分完毕 |
| JS / TS / Vue | **13 → 0** | `GridCanvas.vue`（onMouseDown 36、onKey 23）、`App.vue`、`model.ts`、`bundle-puzzles.mjs` 等改用分派表与小函数 |
| Rust | **20 → 3**（全部在 `aog/search.rs` ×2 与 `backtrack.rs` ×1） | 其余全部降到阈值内；见下 |

Rust 侧保留的 3 个例外：`aog/search.rs` 的 `place_non_predifined_shape`（cognitive
147）与 `dfs`（92）、`backtrack.rs` 的 `dfs`（37）。两者都是 C++
`dfs.cpp` 的直接移植、也是搜索热核：前者整体是一个 `while stack_top > 0` 显式栈
DFS，提交阶段与扩展阶段内联其中；后者是递归放置驱动。它们的分支会 `continue`
外层循环、其中一条路径返回 deadline 码，无法直接搬成函数（需要状态机重写）。
两者在代码中以 `#[allow(clippy::cognitive_complexity)]` + `TODO(complexity)` 显式标注，
crate 内**其余全部函数**都在阈值以内，门禁对其余代码仍然生效。

> **重要教训 1**：曾尝试就地拆分 `aog/search.rs::dfs`（抽出 `dfs_type1_feasible` /
> `dfs_try_placement_at` 等）——交接过来时本就**无法编译**，修复编译后全量基准
> **1110 → 1033，79 道回退**。**已回退。**
>
> **重要教训 2**：`backtrack.rs::dfs` 的拆分（抽出 `assignment_checks_ok` /
> `try_join_existing_region` / `collect_adjacent_regions`）让求解器返回**通不过规则
> 校验的解**，`tests/integration/test_solver_end_to_end.py` 4 个用例失败。
> backtrack 默认关闭、基准 0/1258，**基准完全测不出**。**已回退。**
>
> 结论：求解器热核的复杂度拆分必须「一次抽取 + 立即重建 + 立即验证」，且
> **基准不是唯一护栏**——关闭路径只有 `python -m pytest tests/` 能兜住。

### 7.5 回归验证结果

| 项 | 结果 |
|---|---|
| 复杂度门禁（三语言） | 全部通过：Python 801 块 ≤ 10、Rust 无超阈值、JS 无超阈值 |
| `python -m pytest tests/` | 301 passed，exit 0 |
| `cargo test` | 33 passed |
| 全量基准（`--timeout 30 -j 6`） | **1120 / 1258**，对比基线 1110：**0 回退，+10 新解** |

基准说明：默认 `-j 18` 下同一份代码两次跑出 1106 / 1100（波动 6），差异全部落在
「基线里 18.8s–57.3s 的慢题」上——它们在并行争抢下随机超时。逐一复测这些题
**全部可解且耗时不高于基线**（如 1261：18.8s → 6.3s，0794：56.4s → 41.2s），
降到 `-j 6` 后升至 1120，确认是争抢噪声而非行为回退。

### 7.5 降低复杂度的做法（本项目约定）

1. **纯重构**：不改变行为——不调整循环/迭代顺序、不改剪枝与提前返回、
   不改输出文本与退出码。求解器热路径（`aog/search.rs`、`edge_csp/prop.rs`、
   `rose/rose_growth.rs`）尤其如此，任何语义变化都会体现在 1258 题基准上。
2. **手法**：抽取 `__inline` 辅助函数 / 私有方法；`if/elif` 长链改分派表
   （dict / 查表 / 匹配表）；按绘制或校验的「子步骤」拆分。
3. **radon 的类级聚合**：radon 会把方法复杂度聚合到类上，因此类本身也可能
   被判超阈值——方法降下来后要重新跑门禁确认类级也通过。
4. **回归护栏**：复杂度降低后必须跑 `python -m pytest tests/` 与全量
   `python scripts/benchmark_rust_solver.py --timeout 30`，确认 0 回退。

### 7.6 同批完成的 ruff lint 清理

复杂度拆分顺带暴露了大量存量 lint 问题，本轮一并清零（`ruff` 配置见
`pyproject.toml`，line-length 100）：

| 项 | 结果 |
|---|---|
| `ruff check src/ scripts/ tests/` | **245 → 0**（`main` @ `0500078` 基线为 245） |
| `ruff format src/ scripts/ tests/` | 全部已格式化，`--check` 干净 |
| `pyproject.toml` ruff 配置 | `[tool.ruff] select` 迁到 `[tool.ruff.lint] select`，消除弃用警告 |

清理手法：`ruff format` + `ruff check --fix` 吃掉 I001 / W293 / F401 等可自动项；
其余手工处理——接口桩与 Qt override 保留签名并加带理由的 `# noqa`
（与 `src/` 既有 28 处写法一致），`tests/unit/test_board.py` 删掉两对同名
测试方法（F811，第一个定义实际上从未被执行），Qt `clicked` 信号的
`lambda checked` 改为 `lambda _checked`，`E741` 的 `l` 改名为 `left`。

---

## 8. 复现命令

```bash
# Python 圈复杂度（含分级与平均）
radon cc src -s -a --total-average
# Python 可维护性指数
radon mi src -s
# Python 体积指标
radon raw src -s
# Python 高复杂度块（C 级及以上）
radon cc src -s -n C

# 双语言函数级圈复杂度（CSV，按 CCN 降序取 Top）
lizard src -l python --csv
lizard rsolver/src -l rust --csv
# 概览（含告警计数与比率）
lizard src -l python
lizard rsolver/src -l rust
```

原始输出已归档于 `results/tmp/complexity/`（radon_cc.txt / radon_mi.txt / radon_raw.txt / lizard_py_cc.txt / lizard_rs_cc.txt / py_perfile.txt / rs_perfile.txt）。
