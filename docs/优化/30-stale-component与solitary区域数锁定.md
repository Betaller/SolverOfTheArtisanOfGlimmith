# 30 — 陈旧组件快照与 solitary 区域数锁定

日期：2026-09-20
分支：`feat/solitary-piece-count`
基线：1156/1258（`d80e16d`）

## 0. TL;DR

两件事，同一个诊断会话里挖出来：

1. **一个贯穿 edge_csp 的正确性 bug 类**："传播器写了边，但组件数组没重建，
   下一个传播器拿着陈旧快照做推理"。它让 `propagate_solitary` 的 S3 在
   1017 上把一个**已经合并进有线索区域**的组件当成"封闭且无线索"→
   根层矛盾。修复后 0685 从 FAIL 变 PASS。
2. **`solitary` 的区域数锁定没被 edge_csp 用上**。`docs/rules-guide.md`
   §3.14 明写"计算所有被计数的线索数量 = M，则区域数恰好为 M——这是最强的
   区域数确定方法"，但 `structural_pieces` 只接了 `precise` 和
   `rose_window` 两个来源。补上第三个来源，并新增 D0 规则
   （组件数是区域数的上界）。87/87 官方解验证线索数 == 区域数。

## 1. 诊断方法（可复用）

沿用 29 号文档的两件套，本次又加了两件：

| 手段 | 用途 |
|---|---|
| 官方解 Cut 播种 | 把官方 Cut 边写成 `is_boundary` 重跑。0ms 解出 ⇒ 传播器无辜；仍穷尽 ⇒ 传播器拒绝官方解 |
| `SKIP_AOG=1`（新增） | 跳过 aog/rose，单独跑 edge_csp（对偶 `AOG_ONLY`） |
| `EDGE_CSP_SKIP=a,b,c`（新增） | 按名字关掉传播器，**不用重新编译**就能二分 |
| 逐 `return Err(())` 打行号 | 一次性脚本把 `prop.rs` 里 104 个 `return Err(());` 前面插一行 `eprintln!("... line {}", line!())`，直接定位矛盾点 |

`EDGE_CSP_SKIP` 支持的名字：`bricky` `compass` `compass_in_comp`
`compass_enum` `solitary` `dual` `probe` `area` `areatgt` `areachk`。

### 1.1 一个反复踩的坑：播种脚本必须补齐"同区域边"

第一次播种只把**跨区域**边写进 `pz['edges']`，同区域边没写。结果
`grid.num_edges()` 还是 60，但 JSON 里只有 26 条，adapter 只对 JSON 里的边
做 `is_boundary` 判断——于是"未知边"数量对不上，`cc` 算出来是 10 而不是 4，
白查了半小时。**播种脚本必须把全部内部边都写进 JSON**（同区域的写
`is_boundary: false`），否则 edge_csp 看到的边集合和你以为的不一样。

## 2. 陈旧组件快照

### 2.1 症状

1017（6×6，4 个罗盘 + solitary）官方解 Cut 播种后：

```
edge_csp PROP start: unk=34 cut=26 uncut=0
edge_csp PROP start: unk=28 cut=26 uncut=6
edge_csp PROP start: unk=23 cut=26 uncut=11
edge_csp PROP start: unk=18 cut=26 uncut=16
edge_csp S3: sealed clue-less component ci=2 sz=4 cells=[2, 3, 4, 5]
```

组件 `{2,3,4,5}` = `(0,2)(0,3)(0,4)(0,5)`。官方解里这四格属于区域 1
（含罗盘线索 `(4,3)`）。它被判"封闭且无线索"是假的：`(0,1)-(0,2)` 已经被
`propagate_compass_placement_enumeration` 强制 Uncut 了，组件其实已经和
`(0,1)` 连上——只是 `curr_comp_id` / `comp_cells` / `growth_edges` 还是
`build_components()` 那一刻的快照。

### 2.2 根因

`propagate_area_constraints` 串行调用一串子传播器，每个都读组件数组、都可能
写边：

```
build_components()          ← 快照在这里产生
  areatgt 封边              ← 写 Cut
  inequality / diff
  compass_in_comp           ← 写 Cut/Uncut
  compass_enum              ← 写 Cut/Uncut
  size_separation / boxy
  solitary S4               ← 写 Uncut
  areachk / shape
```

任何一步写了边，后面所有步看到的都是**过期的连通性**。同样的毛病也出现在
`propagate()` 顶层：`propagate_dual_connectivity` 用
`self.curr_comp_sz.len()` 当 `num_comp`，而 `propagate_area_bounds` 内部
已经改过边——D2 在半更新的图上算 `cc`，得到 10 而不是 4，再被新加的
"精确件数"检查当成 `cc > pieces` 假矛盾。

### 2.3 修复

两处：

1. `propagate_area_constraints` 改成"任何子传播器报告 progress 就
   `build_components()` 重建，再跑下一个"。
2. `propagate()` 顶层：`propagate_area_bounds` 返回 progress 时**推迟**
   `propagate_dual_connectivity` 到下一轮不动点（那时组件已重建）。

为此让 `build_components_growth_edges` 把"我写了边"记进新的
`Solver::build_progress` 字段（它之前返回 `Result<(),()>`，写边完全不上报，
不动点循环可能在边已变的情况下提前收敛——顺带修掉的第三个问题）。

代价：progress 高发期每轮多几次 `build_components`（O(格+边)）。不动点收敛
后 progress 为 false，不再多付。

### 2.4 顺带修的两个 soundness bug

**（a）`get_compass_area_bounds` 的 `exact_area` 把未指定方向当 0**

```rust
// 旧
if e == Some(0) && w == Some(0) {
    exact_area = Some(1 + nv + sv);   // nv = n.unwrap_or(0) ← n 为 -1 时算错
}
```

E/W 都是 0 时区域确实全在线索所在列，但列的长度是 `n + s`；只要有一个是
`-1`，总数就未知。旧代码把 `-1` 当 0，低估区域面积 → 后续 `size > target`
假矛盾。

```rust
// 新：四个方向都已知才算 exact
if e == Some(0) && w == Some(0) && n.is_some() && s.is_some() { ... }
else if n == Some(0) && s == Some(0) && e.is_some() && w.is_some() { ... }
```

**（b）`compass_enum` 的门槛用错了上界**

`curr_max_area` 来自四个半平面计数之和，而罗盘半平面互相重叠（东北格同时
算进 N 和 E），这个和**系统性高估**。1017 上 `curr_max_area = 36`，而罗盘
bbox 只有 8–16 格。`MAX_AREA_THRESHOLD = 12` 因此把 4 个线索全跳过了，
`compass_enum` 在未播种的 1017 上根本不跑。

改成用 **bbox 内可填充格数**（区域连通性保证区域 ⊆ bbox，这是可靠上界，且
通常紧得多）：

```rust
let bbox_area = self.count_bbox_cells(bbox);
let max_a = max_a.min(bbox_area);
if max_a > MAX_AREA_THRESHOLD { continue; }
```

## 3. `solitary` 区域数锁定

### 3.1 事实

`validate::check_solitary` 的谓词是"每个区域恰好一个线索格"，线索格 =
symbol / compass / number / shape_pattern / fence_pattern。既然每个线索格
必然落在某个区域里，**区域数 == 线索格数**，双射。

对 87 道官方 `solitary` 题逐一对答案：**87/87 线索数 == 区域数**，零例外。

### 3.2 接进 `structural_pieces`

`Solver::solve()` 里 `structural_pieces` 的第三个来源：

```rust
if self.structural_pieces.is_none() && self.rules.solitary {
    let k = self.clue_cell.iter().filter(|&&b| b).count();
    if k >= 2 { self.structural_pieces = Some(k); }
}
```

`clue_cell` 已经和 `validate::check_solitary` 用同一套谓词，所以计数口径
天然一致。`k >= 2` 是因为 D1/D2/D0 对 1 件数没有可用推理。

### 3.3 新规则 D0：组件数是区域数的上界

组件只增不减——把 Unknown 边设成 Cut 不会拆开已 Uncut 连通的格子，设成
Uncut 只会合并。所以**最终区域数 ≤ 当前组件数**。于是：

- `num_comp < K` ⇒ 矛盾（造不出更多区域）
- `num_comp == K` ⇒ 分区已冻结：任何还需要长大的组件矛盾；所有跨组件的
  Unknown 边必须 Cut

写在 `propagate_dual_connectivity` 里，`num_comp == K` 时直接
`return Ok(progress)`——D1 会合并（非法），D2 的图在这些 Cut 落地后也没有
边可言。

这条对 `precise` / `rose_window` 来源的件数同样适用，不只 `solitary`。

### 3.4 为什么 1017 还是解不出

D0/D2 的收益要等搜索把边切到 `num_comp` 或 `cc` 逼近 K 才兑现。1017 是
6×6、60 条内部边、要切约 26 条，edge DFS 在 40s 内走不到那个点
（1.45M 节点，unknown 还剩 53）。播种 26 条 Cut 后 `cc` 直接到 4，D2 的
`exact == cc` 立刻把剩下 12 条全 Uncut，0 节点解出——说明规则本身是对的、
强的，缺的是搜索序/更早的罗盘剪枝。

`compass_enum` 换 bbox 上界后在未播种 1017 上会激活（(3,1) 的 bbox 只有 8
格），但仍不足以在预算内切出 4 个连通分量。这簇的下一步是把
`MAX_AREA_THRESHOLD` 抬高并配合 `MAX_PLACEMENTS` 上限，或者给 edge DFS
换"优先切 bbox 不相交的格对"的变量序——都留到下一轮。

## 4. 诊断脚手架（保留）

- `SKIP_AOG=1`（`rsolver/src/solver/mod.rs`）：跳过 aog/rose。
- `EDGE_CSP_SKIP=...`（`rsolver/src/solver/edge_csp/prop.rs::csp_skip`）：
  按名关传播器。

两者都只读环境变量、默认关闭，不影响正常路径。逐行 `eprintln!` 的临时打点
已全部删除。

## 5. 验证

- `cargo test --release`：28 + 8 全过
- `python -m pytest tests/`：全过
- `python3 scripts/complexity_gate.py`：通过
- `--rules solitary`：71 → **72**/87（0685 翻正）
- 官方解 Cut 播种下的 1017：修复前根层矛盾，修复后 0 节点解出
- **全量基准 `--timeout 40 -j 6`：1156 → 1157 / 1258**
  - 新解 **0629**（compass+differentiation+ring）：aog 40s 超时后 edge_csp
    **2.3s** 解出——bbox 面积门槛让 `compass_enum` 在这块棋盘上真正跑起来了。
  - 新解 **0685**（compass+heterogeneous/homogeneous+solitary）via aog。
  - 1140fix 在 `-j 6` 下 aog 与 edge_csp 双双 40s 超时翻负，**串行复测两次均
    61s SOLVED**——争抢噪声，非回归（与 memory 里 aog `block_adj` HashMap
    非确定性 + 并行争抢 flaky 的既有结论一致）。

## 6. 追加：S5d 潜在连通性 + 搜索序（同分支第二轮）

D0/D2 的收益要等搜索把组件数（或组件图的 `cc`）压到 K。1017 是 6×6、60 条内部
边、要切约 26 条，edge DFS 在 40s 内走不到那个点。两件事补上这段路：

### 6.1 S5d — 潜在连通性（`solitary_potential_connectivity`）

S5a/S5b/S5c 都不看可达性。新增：

> 对**非 Cut** 边（Uncut 或仍 Unknown）做一次 flood-fill，得到「仍可能连通」的
> 组件。可行集是单点 `{i}` 的格子必须落进区域 `i`，而区域是 Uncut 连通的，所以
> 它必须和线索 `i` 同属一个潜在组件。任何切断所有这种路径的 Cut 都是矛盾。

整盘一次 flood-fill，O(格+边)。只在 `solitary_feasible_active` 时跑。

### 6.2 搜索序（`select_edge`）

端点可行集交集越小，这条边越可能是边界。优先切它，`cc` 就越快爬到 K，D2 的
`exact == cc → 全 Uncut` 和 D0 的冻结规则就能替搜索收尾，而不是把每条剩下的边
都交给 DFS 决定：

```rust
if self.solitary_feasible_active {
    let shared = (self.solitary_feasible[c1] & self.solitary_feasible[c2]).count_ones();
    score += (8 - shared.min(8)) * 6;
}
```

### 6.3 实测

`--rules solitary` 72 → **74**/87；全量基准 **1157 → 1159 / 1258**。

- **1017**（罗盘簇里最小的那道，本次整条诊断线的起点）via edge_csp 解出。
- **1060** via edge_csp 解出。
- 1140fix 在上一轮 `-j 6` 下争抢翻负，本轮并行下也 PASS（噪声消除）。
- 0685 本轮并行翻负，**串行复测两次均 20s SOLVED via aog**（噪声）。

剩余 13 道 solitary FAIL：0308 / 0312 / 0680-0683 / 1080 / 1093 / 1109 /
1246 / 1258 / 1259 / 1260。其中 0680-0683/1258 是 12×13~15×15 的大罗盘题，
1246/1259/1260 是 7×7 五线索——都是搜索深度问题，不是传播缺口。
