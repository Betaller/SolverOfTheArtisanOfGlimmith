# 谜题建模方式对比与求解器数据结构优化方案

> 状态:分析/方案文档,已过一轮独立审查(2026-08-05)。
> 适用范围:Python 求解器(`src/`)、Rust 求解器(`rsolver/`)、第三方参考项目(`third_party/`)。
>
> **更新（2026-08-06）**：文中"现状来源"所引 Python 求解器源码（`backtrack.py` /
> `candidates.py` / `exact_cover/solver.py` 等）已随 Python 求解器栈移除
> （`docs/official-puzzles-status.md` §C.0）。本文的建模对比与性能分析仍可供 Rust 侧
> 数据结构（`rsolver/`）参考，但逐行引用不再对应现有代码。
>
> 审查结论:方案方向正确、可实施性强。关键修正——Cell 实测 192B 与估算一致但逐字段归因有误;`Cell.region_id` 是 Rust 求解路径死字段;P0/P1 风险分别为"低-中"/"中-高";**遗漏了 `pieces.rs` 候选生成(每层克隆 BTreeSet)这一更靠前的热点**,应插在 P0 之后。
>
> 关联文档：`rsolver` 源码详解系列见 [rust-solver/README.md](../rust-solver/README.md)（架构 / 数据结构 / 规则映射 / 四大算法 / 验证）。

## 1. 概述

《格里米斯的工匠》谜题的本质:在一个带洞(`blocked`)的矩形网格上,把所有可填格子划分成若干**四连通区域**(region),同时满足 22 类线索规则。

三方(本仓库 Python 求解器、Rust 求解器、第三方参考求解器)共享**同一个 JSON wire 契约**(`src/io/puzzle_codec.py` ↔ `rsolver/src/main.rs` 的 `PuzzleJson`),这是它们能互通的根基。差异集中在**内部盘面表示**与**求解状态结构**上。

统一约定:
- 格子坐标 0-based `(r, c)`,行从上到下、列从左到右;`(r, c)` 格映射到 aog 加边网格的奇坐标。
- 顶点 `(r, c)` 是格 `(r, c)` 的右下角,即四格 `(r,c),(r,c+1),(r+1,c),(r+1,c+1)` 共用的角。
- 预画边界 `is_boundary=True` 强制相邻格分属不同区域;外边框恒为区域边界。
- 区域 = 共享 `region_id` 的格子集合;区域之间的边界 = 相邻格 `region_id` 不同。

## 2. 建模方式总览

| 建模方式 | 代表 | 盘面表示 | 区域/求解主状态 | 求解算法 |
|---|---|---|---|---|
| 对象图 | Python 模型层 `src/models/` | `Board._cells[r][c]` 对象引用 | `region_id` 写在各 `Cell` 上 | DLX / 回溯 / 玫瑰窗专用 |
| 二维数组 + 结构体 | `rsolver/src/types.rs` | `Vec<Vec<Cell>>` + 边/顶点数组 | `region_id: Option<usize>` 常驻 Cell(求解路径死字段) | aog / pieces(DLX) / backtrack |
| 加边框像素网格 + 位域 | C++ AoG_Solver / rsolver aog | `(2h+5)×(2w+5)` 的 `u32` 数组 | 低 16 位区域号、高 16 位形状号 | 区域填充 DFS + 形状库剪枝 |
| 边状态 CSP | third_party Rust aog | 独立 `EdgeId`/`VertexId` 一维索引 | `edges: Vec<EdgeState>{Unknown,Cut,Uncut}` | 边导向传播器 + DLX |
| BigInt 位掩码 | JS glimmith-solver | cell 单整数索引 `y*w+x` | 候选 region = `{cells, mask: BigInt}` | 位掩码覆盖 DFS |
| 集合模型 | Python TAGSolver | `Grid{cells: Set[Coordinate]}` | 各 piece 的 cell 集合 | 回溯(仅"全等分割" demo) |
| 布尔网格 | TS shape-helper | `Grid = boolean[][]` | 裁剪包围盒 | 拼贴/枚举/分割(辅助工具) |

## 3. 各建模方式的优缺点

### 3.1 对象图模型(Python 模型层)

- **优点**:可读性/可维护性最好;规则检查器统一从 `Board` 读盘(`constraints.py` 全部走 `board.cell()/edges()/vertices()`);`@dataclass(slots=True)`(`board.py:60`)已消除 `__dict__` 额外开销。
- **缺点**:每个 `Cell` 是独立 Python 对象,slots 也需 ~64B 头;热路径上 `board.cell(r,c).region_id` 是多次属性解引用;求解时大量字段(`symbol`/`shape_pattern`/`fence_pattern`/`compass`)对多数格子永远无用却常驻。
- **结论**:适合作为"模型/契约层",不适合作为高频求解状态。

### 3.2 二维数组 + 结构体(Rust `Puzzle`)

- **优点**:内存连续、类型安全、编译器能优化边界。
- **缺点**:`Vec<Vec<Cell>>` 两层堆指针(`cells[r][c]` 两次解引用);`Cell` 结构体实测 **192B**(见 §5.1),且 `symbol: Option<String>` 涉及堆分配;求解热路径实际只用到其中极小部分。
- **结论**:作为"读模型"尚可;一旦进入求解热路径(尤其每格每层都要读写 `region_id`),应把求解状态与线索模型分离。

### 3.3 加边框像素网格 + 位域(C++ / rsolver aog)

- **优点**:area/line/vertex 三种单元交织在**一个** `u32` 数组里,信息局部性最好、内存带宽最优;每个 DFS 深度用预分配的 `PlaceLevel` 定长栈(`rsolver/src/solver/aog/types.rs:224`)避免频繁分配;区域号+形状号打包进一个整数(`SOLVE_AREA_BIT`,低 16 位区域、高 16 位形状)。
- **注意**:单个 u32 数组元素是 4B,但 padded 网格是 `(2h+5)×(2w+5)`(aog/types.rs:205),16×16 实际约 **21B/逻辑格**(~5.4KB),仍紧凑,但勿按"4B/格"理解。
- **缺点**:可读性/可维护性差;新增规则要扩展位域常量;逻辑坐标 → padded 坐标换算成本(索引放大 2×);调试困难。
- **结论**:性能最优,已高度优化(CLAUDE.md 记录 rsolver 已在 aog 内移植了 C++ 的 ring T 字、slash-distance 等额外剪枝,已非严格 1:1)。**不列为本次重构范围**——理由是该模块改动风险大且收益边际低,而非"1:1 可校验性"。

### 3.4 边状态 CSP(third_party Rust aog)

- **优点**:把"切割"显式建模为每条边的 Cut/Uncut;砖纹(4-way)、环纹(3-way)、望塔等顶点/边规则天然表达;传播器(dual/bridge/compass)与启发式选边好写。
- **缺点**:区域由边状态**隐式导出**,需维护连通分量;回溯复制 `Vec<EdgeState>` 成本高;增量更新复杂。
- **结论**:更现代的架构,可作为 rsolver 后续演进方向,但短期重构风险高。

### 3.5 BigInt 位掩码(JS)

- **优点**:覆盖/交集/子集判断近乎 O(1) 位运算;候选去重简单。
- **缺点**:BigInt 仅 JS 生态;网格大时位宽膨胀;可读性差。
- **结论**:浏览器场景的巧妙取舍,不值得移植。

### 3.6 HashMap 部分解状态(Python + rsolver backtrack)

- **优点**:增量更新灵活(只动当前区域)。
- **缺点**:哈希开销大;`(usize, usize)` 元组 key 缓存不友好;Python 侧还伴随 dict/set 的**每次递归复制**。
- **结论**:两种语言 backtrack 中最明显的低效点,优化收益确定但**天花板有限**(backtrack 是调度链最后兜底,多数谜题走不到,见 §6 P0)。

## 4. 当前 Python 求解器数据结构诊断

现状来源:`src/solver/backtrack.py`、`src/models/board.py`、`src/solver/exact_cover/solver.py`、`src/solver/candidates.py`。

| # | 低效点 | 位置 | 说明 |
|---|---|---|---|
| P1 | 每次递归**复制整个 regions dict** | `backtrack.py:242` `new_regions = {**regions, rid: region_cells}` | DFS 深度 × 分支数 全量 dict 复制 |
| P2 | 每次递归**重建 unassigned set** | `backtrack.py:241` `unassigned - region_cells` | 集减法生成新 set |
| P3 | 区域用 `set[tuple[int,int]]` | `backtrack.py:208` | tuple 创建+哈希;可编码 `r*w+c`(但会渗透到 candidates.py/checks.py/constraints.py,改动量接近 P1,见 §6 P2) |
| P4 | `board.cell(r, c)` 逐格属性链 | `backtrack.py:239` 等 | 热路径每格多次解引用 |
| P5 | 候选生成中 set 并集复制 | `candidates.py:447/496` `current \| new_cells` | 每个枚举节点都复制 |

> **审查补充:Python 侧真正的瓶颈是候选生成**(`candidates.py:365-500` 的 `_enumerate_regions` 每个节点做 set 并集复制并重新 `_region_feasible`;`_remaining_capacity_ok`(`backtrack.py:281`)每次放置后全量 `_get_all_components` BFS)。**因此 P2 的"消除 dict/set 拷贝"很可能是低 ROI,必须先用 benchmark 确认**,这符合文档把 P2 标为"收益存疑"的判断。

## 5. 当前 Rust 求解器数据结构诊断

现状来源:`rsolver/src/types.rs`、`rsolver/src/solver/backtrack.rs`、`rsolver/src/solver/pieces.rs`。

### 5.1 `Cell` 结构体过大(`types.rs:88-99`)

审查代理用 `rustc` 实测(64 位):`Cell` 总大小 **192B**(align 8)。

| 字段 | 类型 | 实测字节 | 说明 |
|---|---|---|---|
| `row` / `col` | `usize` | 16 | 瘦身可删(由索引推导) |
| `number` | `Option<i64>` | 16 | Rust 对无 niche 整数不压缩 tag;改 `Option<NonZeroU8>` 可省 ~15B |
| `symbol` | `Option<String>` | 24 | String 24B(ptr+len+cap,非空指针 niche);堆分配 |
| `blocked` | `bool` | 1(+7 padding) | 可并入位标志 |
| `compass` | `Option<CompassClue>` | 64 | CompassClue=4×Option<i64>;内部 tag 提供了 niche,Option 不额外膨胀 |
| `fence_pattern` | `Option<Shape>` | 24 | Shape=Vec=24B |
| `shape_pattern` | `Option<Shape>` | 24 | 同上 |
| `region_id` | `Option<usize>` | 16 | **求解路径死字段**(见下) |

16×16 = 256 格 × 192B ≈ **48KB/每 Puzzle 实例**,与文档原始"~50KB"估算一致(但逐字段归因修正:symbol/shape 是 24B 非 32B,`Option<i64>` 是反直觉的 16B)。

**关键发现:`Cell.region_id` 是 Rust 求解路径的死字段(16B/格)**。全仓 grep `region_id` 显示,唯一读取 `Cell::assigned()`(`types.rs:117`)的 `grid::unassigned_cells`(`grid.rs:15-26`)**从未被任何求解器调用**;backtrack 用自己的 `state.cell_to_region`(`backtrack.rs:37`),pieces 用 `cell_to_idx`,aog 用 u32 位域。这比"线索与状态分离"的直觉更硬——**这 16B 可直接从 Cell 删除**。

### 5.2 `Vec<Vec<Cell>>` 两层间接(`types.rs:154`)

`cells[r][c]` 需两次解引用。扁平化 `Vec<Cell>` + `idx = r*w+c` 即可,`h_edges`/`v_edges`/`vertices` 同理(长度分别 `h*(w-1)`、`(h-1)*w`、`(h-1)*(w-1)`,见 §6 P1)。

### 5.3 backtrack 的 HashMap 状态(`backtrack.rs:36-44`)

```
struct BacktrackState {
    cell_to_region: HashMap<(usize, usize), usize>,
    region_shapes:  HashMap<usize, Vec<[usize; 2]>>,
    ...
}
```

每格一个 `(usize, usize)` 元组哈希。区域 id 由 `next_region_id` 递增分配(0..n),天然可作数组下标,无需 HashMap。

**审查补充的其他热点(比哈希更大,且 P0 可顺手改)**:
- `check_watchtowers_ok`(`backtrack.rs:305-320`):对**每次单元格赋值都遍历全部望塔**,O(#cells × #watchtowers)。
- `check_vertex_ring_ok`(`backtrack.rs:344-345`):每次调用扫两遍 `puzzle.rules` 算 `has_ring/has_brick`,可缓存到 `BacktrackState`。

### 5.4 aog 位域网格(`solver/aog/core.rs`)

已是紧凑 `u32` 位域 + 深度池化,性能最优,不列为本次优化项。

### 5.5 pieces 候选生成(`solver/pieces.rs`)—— 最大遗漏热点

`poly_rec`(`pieces.rs:371-419`)每层 `let mut my_candidates = candidates.clone()`(388)克隆整个 `BTreeSet<[usize;2]>`,每结果 `current.clone()`(379),且 `current.contains(&pos)`(403)是线性扫描;`compass_rec`(`pieces.rs:451-563`)同样每层 clone(500)。这是 **Rust 侧比 P0 哈希更明显的热点**,且 pieces 在调度中排在 backtrack 之前(优先路径),形状/面积/指南针类谜题直接命中。

## 6. 优化方案

### P0(低-中风险 · 中收益 · 可先行):Rust backtrack 去哈希

**目标**:消除 backtrack 的 HashMap 开销,顺手清理望塔/环砖检查热点。
**改动点**:
1. `cell_to_region: HashMap<(usize,usize), usize>` → `Vec<Option<usize>>`,长度 `h*w`,下标 `r*w+c`(`backtrack.rs:37`)。语义等价:blocked/未赋值格 → `None`;`contains_key`(142,362)→`[idx].is_some()`;`get`(160-195, 309, 329, 407, 431)→`[r*w+c]`。
2. `region_shapes: HashMap<usize, Vec<[usize;2]>>` → `Vec<Vec<[usize;2]>>`,区域 id 直接作索引。**push/pop LIFO 不变式成立**:`new_rid=next_region_id`(265)、`insert`→`push`(268)、`remove(&new_rid)`→`pop`+`next_region_id-=1`(277-278),id 恒为 0..len-1。唯一要求 `region_shapes[rid].push` 时 `rid<len`,`valid_rids` 只来自已赋值邻居,成立。
3. **唯一需改签的函数**:`check_merge_ok` 的 `assigned: &HashMap<(usize,usize),usize>`(`backtrack.rs:284-302`)及其调用点 246。`grid::is_adjacent_free` **不受影响**(只读 puzzle 边)。
4. 顺手:`has_ring/has_brick` 缓存进 `BacktrackState`;`check_watchtowers_ok` 改为先收集待查望塔再遍历。
5. 遍历 fillable 格改走线性索引为**可选项**(若不改,只需在 ~30 处访问点收敛到 `r*w+c`)。

**注意陷阱**:
- 4 处回滚 continue 点(250-251 之后、254、270 的失败)必须逐一保留 pop/None 复位,是机械改动中最易漏的点。
- **backtrack.rs 目前没有单测**(`#[cfg(test)]` 只在 `aog/core.rs` 与 `constraints.rs`),`cargo test` 跑不通该路径。建议 P0 一并补 3-5 个 backtrack 单测(区域划分/望塔/环砖/边界),否则回归只能靠 Python 侧 `verify_puzzles.py`。

**预期收益**:消除每格哈希 + 望塔/环砖检查去重。真实但**天花板有限**——backtrack 是 `solver/mod.rs:30-74` 调度链的最后兜底,多数谜题走不到。故评级"中收益"而非"高收益"。
**验证**:`cd rsolver && cargo test`(新增单测)+ `scripts/verify_puzzles.py` 全量回归。

### P0.5(中风险 · 高收益 · 优先路径):pieces 候选生成去克隆

**目标**:消除 `poly_rec`/`compass_rec` 每层的 `BTreeSet` 克隆与线性 `contains`。
**改动点**:
1. `pieces.rs:388` `candidates.clone()` → 用迭代器借用 + 显式回溯(候选集只增删当前位置,而非每层复制)。
2. `pieces.rs:403` `current.contains(&pos)` → 用 `bool` 占用掩码(flat `Vec<bool>` 或 u64 位集)代替线性扫描。
3. `pieces.rs:379` `current.clone()` → 复用可变 buffer,递归后恢复。
4. (可选)`polyomino.rs::transforms`(`polyomino.rs:6-44`)对形状池的结果做缓存,避免每个起点重建 8 个变换+去重。

**预期收益**:pieces 是调度链中排在 backtrack 之前的优先路径,形状/面积/指南针类谜题直接命中;消除每节点克隆的分配与线性扫描。
**风险**:中。改动候选生成热路径,需保证生成结果与现状完全一致。
**验证**:`cargo test`;`scripts/verify_puzzles.py --timeout 30` 对照基准,重点看 shape_pool/area/compass 类谜题。

### P1(中-高风险 · 中高收益):Rust `Puzzle` 扁平化 + `Cell` 瘦身

**目标**:降低模型占用与访问间接,将"线索"与"求解状态"分离。
**改动点**:
1. `Puzzle.cells: Vec<Vec<Cell>>` → `Vec<Cell>`;`h_edges/v_edges/vertices` 同步压平。索引辅助:`idx(r,c)=r*w+c`、`h_edge_idx(r,c)=r*(w-1)+c`、`v_edge_idx(r,c)=r*w+c`、`vertex_idx(r,c)=r*(w-1)+c`。
2. `Cell` 瘦身:
   - **删除 `region_id`**(16B,求解路径死字段,见 §5.1)。
   - `symbol: Option<String>` → `Option<u8>`(ASCII/内部编码)或全局符号表索引;`aog build` 的 `rose_types.iter().position(|t| t==sym)`(`aog/core.rs:700-706`)改后可直接用索引。
   - `shape_pattern` / `fence_pattern` 从每个 Cell 移除,改为 `puzzle` 级线索表(`Option<u32>` 索引)。
   - `compass` 抽全局线索表(固定 64B,影响最大)。
   - `number: Option<i64>` → `Option<NonZeroU8>`。
   - 估算:Cell 由 192B 降至 **~40-60B**,16×16 从 ~48KB 降到 ~13KB。
3. 全仓适配读取点。

**波及面(比初稿多)**:
- 构造:`main.rs:180-228`。
- 读取:`grid.rs:7-10`(`is_adjacent_free`)、`solver/mod.rs:99,116`(`regions_respect_boundaries`)、`solver/backtrack.rs:109,465-535`、`solver/pieces.rs:162,182-203,568-571`、`solver/aog/core.rs:660-769`(build)、**`solver/validate.rs:48,59,261,539-552`**(aog 解的独立校验器,漏改会出大问题)。
- `aog/search.rs` 与 `aog/empty.rs` **不读 Puzzle**,扁平化不影响 aog 搜索热路径(好消息)。

**预期收益**:内存占用下降 ~3.5×;访问局部性提升;aog 之外的求解器受益。
**风险**:中-高。动核心模型,波及面含 `grid.rs`/`mod.rs`/`solver/validate.rs`/aog build;Cell 瘦身与 aog build 强耦合,必须全量回归。
**验证**:`cargo test` + `cargo clippy`;`python -m pytest tests/`;`scripts/verify_puzzles.py --timeout 30`;对照 `scripts/benchmark.py` 基线。

### P2(中风险 · 收益存疑 · 必须先测量):Python 回溯复制优化

**目标**:消除 dict/set 每次递归复制。
**改动点**:
1. `regions` 改为**单例可变 dict**:赋值 `regions[rid] = region_cells` → 递归 → 失败时 `del regions[rid]`,替代 `{**regions, ...}`(`backtrack.py:242`)。
2. `unassigned` 改为**单例可变 set**:递归前 `unassigned.difference_update(region_cells)`,失败回滚 `unassigned.update(region_cells)`,替代集减法(`backtrack.py:241`)。
3. (可选)格子 int 编码 `cell = r*w+c` —— **注意**:这会渗透到 `candidates.py`(`_frontier`/`_enumerate_regions` 全用 tuple)、`checks.py`(`_get_adjacent_region_ids`)、`constraints.py`(所有 `board.cell(r,c)`),改动量接近 P1,**建议单独成档**。

**注意陷阱(审查补充)**:
- **回滚点共 4 处,不是 1 处**:3 个前置 continue(`backtrack.py:245,249,253`)+ 递归返回 None(260)。每处都要同步 `del regions[rid]` + `unassigned.update(region_cells)` + `_unassign`,漏一处即状态污染。
- **别名脆弱性**:成功路径短路返回时不回滚。当前调用方都安全(顶层 `all_positions`(161)成功/失败后均不再用;rose 种子循环的 `sub_unassigned` 每轮重建(127);`_solve_rose_parallel` 各线程独立(562)),但这个安全**依赖调用方不复用**,改 `_search` 时需在 docstring 显式声明该前置不变式。

**前置条件**:先跑 `scripts/benchmark.py` 采样,确认回溯确实占时(而非候选生成)。审查认为 Python 真瓶颈在候选生成(§4 P5),故 P2 很可能是低 ROI。
**验证**:`python -m pytest tests/`;`scripts/verify_puzzles.py --dir puzzles/official --timeout 30`。

### 不建议动:aog 位域网格

该模块已高度优化(紧凑位域 + 深度池化 + 已移植 C++ 额外剪枝),改动风险大且收益边际低。若未来想演进,方向是 §3.4 的"边状态 CSP",那是独立大项目。

## 7. 实施前必读的基线步骤

1. **建立基准**:`cd rsolver && cargo build --release` 后跑 `python scripts/verify_puzzles.py`(30s 超时)与 `python scripts/benchmark.py`,记录全量通过率与耗时。
2. 每档改动**独立提交**,便于二分定位回归。
3. 每档改动后重跑第 1 步,对照基线。
4. P1 实施前用 `std::mem::size_of::<Cell>()` 实测(当前 192B)确认瘦身后效果。

## 8. 建议路线

| 顺序 | 动作 | 说明 |
|---|---|---|
| 1 | `scripts/benchmark.py` 采样 | 确认 Rust/Python 各自瓶颈,给 P2 提供依据 |
| 2 | P0(Rust backtrack 去哈希 + 望塔/环砖缓存) | 低-中风险,先行落地;**一并补 backtrack 单测** |
| 3 | P0.5(pieces 候选生成去克隆) | 优先路径热点,中风险高收益 |
| 4 | 视基准数据决定 P1 或 P2 | Rust 侧优先;P2 仅在基准证明回溯占时后做 |
| 5 | 全量回归 + 提交 | 对照 §7 基线 |

## 附录:关键文件速查

| 关注点 | 文件:行 |
|---|---|
| Python 模型对象图 | `src/models/board.py:60-100` |
| Python 回溯状态与复制 | `src/solver/backtrack.py:208-262` |
| Python 候选生成(真瓶颈) | `src/solver/candidates.py:365-500` |
| Python DLX 建模 | `src/solver/exact_cover/solver.py:51-69` |
| JSON wire 契约 | `src/io/puzzle_codec.py:52-173` |
| Rust 模型结构(Cell 192B) | `rsolver/src/types.rs:88-167` |
| Rust backtrack 状态与回滚 | `rsolver/src/solver/backtrack.rs:36-44,208-302` |
| Rust pieces 候选生成(克隆热点) | `rsolver/src/solver/pieces.rs:371-563` |
| Rust aog 位域网格 | `rsolver/src/solver/aog/core.rs:12-37,660-769` |
| Rust DLX | `rsolver/src/dlx.rs:5-27` |
| C++ 参考(位域) | `third_party/AoG_Solver/src/defines.h:9-49` |
| third_party Rust aog(边 CSP) | `third_party/aog/src/types.rs:6-10`、`grid.rs:65` |
