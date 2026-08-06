# 09 · 附录：术语表、.puz 格式、常量与调试

> 阅读对象：所有读者。本附录是系列文档的公共参考。

---

## 1. 术语表

### 1.1 基本概念

| 术语 | 中文 | 含义 |
|---|---|---|
| Cell | 单元格 | 网格上的一个格子，`(row, col)` 0-based |
| Region / Piece | 区域 / 拼块 | 一组四连通的单元格，共享 `region_id` |
| Edge | 边 | 相邻两格之间的分界线；`is_boundary=true` 表示预画分界线 |
| Vertex | 顶点 | 四格共用角；可带望塔数字 |
| Blocked | 障碍格 | 不参与划分的格子（`#`） |
| Shape | 形状 | 区域格子的相对坐标集合；旋转/镜像视为同一形状 |
| Clue | 线索 | 网格上的提示：格线索 / 边线索 / 顶点线索 |

### 1.2 求解器

| 术语 | 含义 |
|---|---|
| aog | AoG 风格 DFS 求解器（C++ 参考求解器的 1:1 移植） |
| pieces | 基于 DLX 精确覆盖的求解器 |
| backtrack | 区域回溯兜底求解器 |
| rose | 玫瑰窗专用求解器（region_match + rose_growth） |
| DLX | Dancing Links（Knuth Algorithm X 的实现） |
| MRV | Minimum Remaining Values，最少剩余值启发式 |

### 1.3 结构

| 术语 | 含义 |
|---|---|
| `Puzzle` | 领域模型（`types.rs`） |
| `sp` | solve puzzle，aog 搜索时的位域盘面 |
| `PlaceLevel` | aog 每层搜索的工作区 |
| `Pools` | 预分配的 `PlaceLevel` 数组（每 DFS 深度一份） |
| `CellSet` | rose 用的 `Vec<u64>` 位集 |
| `PreBoundaries` | rose 用的预画边界集合 |

### 1.4 规则（22 条）

| ID | 内部名 | 中文 | ID | 内部名 | 中文 |
|---|---|---|---|---|---|
| 1 | `precise` | 精确 | 12 | `range` | 范围 |
| 2 | `shape_pool` | 形状池 | 13 | `solitary` | 独居 |
| 3 | `rose_window` | 玫瑰窗 | 14 | `differentiation` | 差异化 |
| 4 | `homogeneous` | 双生 | 15 | `block` | 方块 |
| 5 | `heterogeneous` | 异生 | 16 | `non_block` | 非方块 |
| 6 | `puzzle_piece` | 拼块 | 17 | `brick` | 砖纹 |
| 7 | `mixed` | 混合 | 18 | `ring` | 环纹 |
| 8 | `area` | 面积数字 | 19 | `inequality` | 不等号 |
| 9 | `fence` | 围栏 | 20 | `difference` | 差值 |
| 10 | `same` | 相同 | 21 | `watchtower` | 望塔 |
| 11 | `different` | 相异 | 22 | `compass` | 罗盘 |

---

## 2. `.puz` ASCII 格式

`aog_puzzles/` 下存放 `.puz` 文本文件（AoG_Solver 参考项目的谜题格式）。
本系列文档中的谜题样例采用同一套 ASCII 约定。

### 2.1 文件结构

```
VERSION 1
PUZZLE_VERSION 2
DIFFICULTY 1
SHAPE 1 3            ← SHAPE <索引> <行数>，后接 <行数> 行 '#/空格'
#..
###
SHAPE_BANK 2
DIMENSIONS 5 4       ← DIMENSIONS <宽> <高>
PUZZLE               ← 谜面 ASCII 网格
+--+--+--+--+--+
|..|..|..|..|..|
+--+==+==+--+--+
...
SOLUTION             ← 题解 ASCII 网格（可省略）
+##+##+##+##+##+
...
```

### 2.2 网格绘制约定

网格有 `2H+1` 行（`H` 行格 + `H+1` 行边/顶点），每格占 **2 字符**宽：

```
   顶点   ← 边行上的 '+' 处
+--+--+--+   ← 边行：'+--+' = 分界线；'+  +' = 连通
|..|..|..|   ← 格行：'|' = 垂直分界线；' ' = 连通；'.' = 空格；'#' = 障碍
```

**格内容**（格行，每格 2 字符）：

| 内容 | 含义 |
|---|---|
| `..` | 普通空格 |
| `  `（两空格） | 障碍格（blocked） |
| 两位数字 | 面积数字线索 |
| `S<n>` | 拼块形状索引 |
| `P<n>` | 玫瑰窗符号类型 |
| `F<n>` | 围栏类型 |
| `U...` | 罗盘线索 |

**边标记**（`|` / `-` / `=` / `!` / `<` `>` `^` `v` / 数字）：

| 字符 | 含义 |
|---|---|
| `|` / `-` | 普通边 |
| `=` | 双生（同形状） |
| `!` | 异生（异形状） |
| `<` / `^` | 不等号：上/左区域更小 |
| `>` / `v` | 不等号：上/左区域更大 |
| 数字 | 差值（值 = 数字 + 1） |

**顶点**：`+` 处的字符若为 `1..4`，表示望塔。

### 2.3 一个完整样例（Zone1/3-gemini-delta/0095）

```
DIMENSIONS 5 4          ← 5 列 × 4 行
PUZZLE
+--+--+--+--+--+
|..|..|..|..|..|
+--+==+==+--+--+        ← (1,1)-(1,2)、(1,2)-(1,3) 是双生边
|..#..|..=..|..|
+--+--+##+--+--+        ← 中间两格是障碍格
|..#..#..|..|..|
+--+##+--+--+--+
|..|..|..|
+--+--+--+
SOLUTION
+##+##+##+##+##+
#     #     #  #
+  +##+##+  +  +
#  #     #  #  #
+##+  +##+##+  +
#  #  #     #  #
+  +##+  +##+##+
#     #  #
+##+##+##+
```

> 题解里：`#` 是已填区域，空格 / `+  +` 是区域边界。相邻不同 `#` 笔画自然形成分区。

---

## 3. 位域常量速查

### 3.1 LINE（边）

| 常量 | 值 | 含义 |
|---|---|---|
| `LINE_NORMAL` | `0x0000_0000` | 普通边 |
| `LINE_BLOCK` | `0x8000_0000` | 分界线（强制两侧不同区域） |
| `LINE_DIFFERENT` | `0x4000_0000` | 异生：两侧形状不同 |
| `LINE_EQUAL` | `0x2000_0000` | 双生：两侧形状相同 |
| `LINE_SMALLER` | `0x1000_0000` | 上/左区域更小 |
| `LINE_LARGER` | `0x0800_0000` | 上/左区域更大 |
| `LINE_SIZE_DIFF_BIT` | `0x000f_0000`（位移 16） | 差值（存 `value+1`） |

### 3.2 AREA（格）

| 常量 | 值 | 含义 |
|---|---|---|
| `AREA_NORMAL` | `0` | 普通格 |
| `AREA_BLOCK` | `0x8000_0000` | 障碍格 |
| `AREA_PALISADE_INDEX_BIT` | `0x7000_0000`（位移 28） | 围栏类型 |
| `AREA_SHAPE_INDEX_BIT` | `0x0f00_0000`（位移 24） | 拼块形状目录索引 |
| `AREA_SHAPE_SIZE_BIT` | `0x00ff_0000`（位移 16） | 面积数字 |
| `AREA_SLASH_INDEX_BIT` | `0x0000_f000`（位移 12） | 玫瑰窗符号类型 |
| `AREA_COMPASS_ENABLE` | `0x0000_0800` | 带罗盘 |
| `AREA_SYMBOL_BIT` | `0x0000_0004` | 带符号字符串 |

### 3.3 SOLVE / VERTEX

| 常量 | 值 | 含义 |
|---|---|---|
| `SOLVE_AREA_SHAPE_INDEX_BIT` | `0xffff_0000`（位移 16） | 求解时：形状索引 |
| `SOLVE_AREA_BIT` | `0x0000_ffff` | 求解时：区域号 |
| `VERTEX_RADAR_BIT` | `0x0000_000f`（位移 0） | 望塔值 |

---

## 4. 运行与调试

### 4.1 构建与运行

```bash
cd rsolver && cargo build --release     # 构建（Python 侧 RustSolver 依赖此二进制）

echo '<puzzle_json>' | ./target/release/rsolver       # stdin 输入
./target/release/rsolver puzzle.json                   # 文件输入
printf '<p1>\n<p2>\n' | ./target/release/rsolver --batch   # 多行 JSON → 逐行题解
```

### 4.2 环境变量

| 环境变量 | 效果 |
|---|---|
| `AOG_DEBUG=1` | 打印求解器选择、aog 构建信息、形状插入、提交格等调试日志（stderr） |
| `AOG_ONLY=1` | aog 失败后**不再**尝试 rose/pieces/backtrack，直接返回 unsolved |
| `RUST_BACKTRACE=1` | panic 时打印堆栈（Rust 通用） |

```bash
AOG_DEBUG=1 ./target/release/rsolver puzzles/official/A/A1-1.json
```

### 4.3 验证一条命令

```bash
python scripts/verify_puzzles.py --dir puzzles/official/A --timeout 30
# 会走 default_router()，即 RustSolver 优先，再独立验证每个解
```

### 4.4 常见调试问题定位

| 症状 | 排查方向 |
|---|---|
| aog 返回 None | 看 `AOG_DEBUG` 日志里的 `aog build: ...`、`dfs index=...`、`aog: internal validation rejected solution` |
| 解跨越预画边 | `build_solution` 里 `regions_respect_boundaries` 拒绝，stderr 会打印 `boundary-violate h/v (...)` |
| rose 不生效 | `is_rose_capable` 要求无 same/different 规则；`rose: region_match start` 日志确认进入 |
| 某个求解器没被尝试 | 检查 `solve()` 路由条件（`has_shape_pool` / `has_area_clues` / `has_compass_clues`） |

---

## 5. 各模块行数一览（2026-08 快照）

| 文件 | 行数 | 文档 |
|---|---|---|
| `solver/aog/search.rs` | 1333 | 04 |
| `solver/aog/core.rs` | 993 | 04 |
| `solver/aog/empty.rs` | 829 | 04 |
| `solver/validate.rs` | 634 | 08 |
| `solver/pieces.rs` | 634 | 05 |
| `solver/backtrack.rs` | 811 | 06 |
| `solver/rose/region_match.rs` | 574 | 07 |
| `solver/rose/rose_growth.rs` | 584 | 07 |
| `shapes.rs` | 184 | 02 / 08 |
| `constraints.rs` | 336 | 08 |
| `main.rs` | 93 | 01 |
| `io.rs` | 264 | 01 |
| `dlx.rs` | 270 | 05 |
| `types.rs` | 221 | 02 |
| `solver/mod.rs` | 238 | 01 |
| `solver/rose/cells.rs` | 198 | 07 |
| `solver/rose/mod.rs` | 188 | 07 |
| `grid.rs` / `polyomino.rs` | 74 | 02 |

---

## 6. 相关文档

- 规则中文详解：`docs/rules-guide.md`
- 求解器路由与 Python 侧：`docs/architecture.md`
- 官方题进度 / 软门禁：`docs/official-puzzles-status.md`
- 建模方式对比：`docs/重构/data-structures.md`
- rose 下沉设计：`docs/重构/rose-solver-rust-port.md`

---

（本系列完）
