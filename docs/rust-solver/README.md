# rsolver 源码详解——文档系列

> 本系列面向《格里米斯的工匠》(The Artisan of Glimmith) 的 **Rust 求解器 `rsolver`**，
> 目标是把 `rsolver/` 下约 8800 行 Rust 代码讲清楚：**小白能看明白、信息完备、配有
> 算法流程图、数据结构图、规则与代码映射表**。
>
> 谜题与题解一律使用 **ASCII 网格**（参考 `.puz` 文件约定）展示，见
> [09-附录-术语表与puz格式.md](09-附录-术语表与puz格式.md)。
>
> 更新日期：2026-08-06

---

## 0. 一分钟看懂这个求解器

`rsolver` 是一个**命令行子进程**：标准输入喂一份谜题 JSON，标准输出吐一份题解 JSON。
它内部串联了 **4 个求解算法**，按顺序尝试，谁先出答案就用谁：

```
谜题 JSON ──► 路由 solve() ──┬─► ① AoG DFS     （形状放置 + 强力剪枝，主力）
                             ├─► ② Rose        （纯玫瑰窗专用）
                             ├─► ③ Pieces(DLX) （形状池 / 面积数字 / 罗盘 → 精确覆盖）
                             └─► ④ Backtrack   （区域回溯，兜底）
                                        │
                                        ▼
                             独立验证 → 题解 JSON
```

- ① `aog`：把 C++ 参考求解器 **1:1 移植**，对**绝大多数**官方题最快。
- ② `rose`：专治 aog 超时的“无尺寸约束玫瑰窗题”。
- ③ `pieces`：把“放形状”变成**精确覆盖**问题，用 Dancing Links 求解。
- ④ `backtrack`：最简单可靠的回溯，兜底一切。
- 任何答案都必须通过**独立验证器**（`solver/validate.rs`）才允许上报，防止错误答案“走私”出去。

---

## 1. 文档地图

| 文档 | 内容 | 难度 | 读完能回答 |
|---|---|---|---|
| [01-总体架构](01-总体架构.md) | 系统全景：路由、JSON 协议、数据流、模块图、时间预算 | ★★☆ | 求解器怎么连起来的？JSON 长什么样？ |
| [02-数据结构](02-数据结构.md) | `Puzzle` 模型 + 四种内部盘面表示 + 坐标换算 + 位域编码 | ★★☆ | 一个谜题在内存里长什么样？ |
| [03-规则与代码映射](03-规则与代码映射.md) | 22 条规则 → 四个求解器各自怎么实现 → 具体代码位置 | ★★★ | 某条规则在 Rust 里在哪实现？ |
| [04-aog求解器](04-aog求解器.md) | AoG DFS：形状库、起手选择、形状放置、空区分析、全部剪枝 | ★★★★ | 为什么 aog 又快又准？ |
| [05-pieces求解器](05-pieces求解器.md) | Dancing Links 精确覆盖：候选生成、矩阵构建、回跳验证 | ★★★ | DLX 怎么解这道题？ |
| [06-backtrack求解器](06-backtrack求解器.md) | 区域回溯：逐格归属、增量约束、叶子校验 | ★★☆ | 最朴素的解法是什么？ |
| [07-rose求解器](07-rose求解器.md) | 玫瑰窗：region_match 精确覆盖 + rose_growth 生长修复 | ★★★ | 玫瑰窗题怎么解？ |
| [08-验证与约束检查](08-验证与约束检查.md) | `solver/validate.rs`（全 22 规则）/ 边界尊重，两套校验各管什么 | ★★☆ | 错误答案怎么被拦下的？ |
| [09-附录-术语表与puz格式](09-附录-术语表与puz格式.md) | 术语表、`.puz` ASCII 格式、位域常量表、调试环境变量 | ★☆☆ | 这些图例/常量什么意思？ |
| [10-拼块优化方向](10-拼块优化方向.md) | puzzle_piece 拼块规则优化：171 题分类、当前路径瓶颈、5 个方向 | ★★★ | 拼块题怎么更快？ |
| [../优化/09-rose-puzzle-piece优化调研.md](../优化/09-rose-puzzle-piece优化调研.md) | puzzle_piece + rose_window 混合优化：10 题分析、搜索空间 10⁶ 缩减、4 阶段实施 | ★★★ | 拼块和玫瑰窗怎么合力？ |
| [TODO.md](TODO.md) | rsolver 优化待办清单（性能/内存/IO/架构，P0~P3） | — | 后续要做哪些优化？ |
| [../优化/README.md](../优化/README.md) | **内存优化专项**：实测基线、根因、方案、泄露审计 | ★★★ | 内存去哪了？怎么优化？ |

---

## 2. 建议阅读路径

- **只想知道整体**：读 01 → 03（规则总表）→ 08，即可。
- **想理解主力算法**：01 → 02 → 03 → 04，把 aog 啃透。
- **想完整掌握**：按 01 → 09 顺序全部读一遍，04 / 05 / 07 可配合源码逐行对照。
- **想调试某个谜题**：01（运行方式）+ 09（调试环境变量与 .puz 示例）。

每篇文档末尾有「本节代码索引」，把文中提到的函数名精确到 `文件:行号`，
方便跳回源码核对。

---

## 3. 源码文件与文档对照

```
rsolver/
├── Cargo.toml                    # 依赖：serde / serde_json
├── src/
│   ├── main.rs                   # 入口：读 stdin/argv → 调 io::* → 写 stdout
│   ├── io.rs                     # JSON 模型、build_puzzle、序列化、--batch 逐行求解
│   ├── types.rs                  # 领域模型：Puzzle/Cell/Edge/Vertex/Shape/...
│   ├── grid.rs                   # 网格工具：可填格枚举、相邻判断
│   ├── shapes.rs                 # 共享形状/面积辅助：dihedral_key / is_rectangle / collect_pool_shapes / area_bounds / rose_symbol_types
│   ├── dlx.rs                    # Dancing Links 通用精确覆盖引擎（pieces 用）
│   ├── polyomino.rs              # 多连块旋转/镜像/生成
│   ├── constraints.rs            # （已删 2026-08-06）原 stub 校验并入 solver/validate.rs
│   └── solver/
│       ├── mod.rs                # 路由 solve() + 答案构建 + 边界尊重检查
│       ├── validate.rs           # 全 22 规则独立校验（aog 出口 + pieces/backtrack 验收 + rose 验收共用）
│       ├── aog/                  # ① AoG DFS（1:1 移植 C++ AoG_Solver）
│       │   ├── mod.rs            #   solve_aog 入口 / 区域提取
│       │   ├── core.rs           #   AoGCore 构建、形状目录、棋盘检查函数
│       │   ├── search.rs         #   dfs() 主递归 + 形状放置迭代 DFS
│       │   ├── empty.rs          #   空区分析 + 起手选择
│       │   └── types.rs          #   常量、PlaceLevel、Pools、位域定义
│       ├── pieces.rs             # ③ Pieces(DLX) 精确覆盖求解器
│       ├── backtrack.rs          # ④ 区域回溯求解器
│       └── rose/                 # ② 玫瑰窗求解器（Python 移植）
│           ├── mod.rs            #   solve_rose 入口 / 符号类型 / M
│           ├── cells.rs          #   CellSet 位集、PreBoundaries
│           ├── region_match.rs   #   region_match（候选 + MRV 精确覆盖）
│           └── rose_growth.rs    #   rose_growth（生长 + 修复）
```

> 与 4 篇算法文档一一对应：`aog/` ↔ 04、`pieces.rs` ↔ 05、
> `backtrack.rs` ↔ 06、`rose/` ↔ 07。
> 优化方向：10 ↔ 拼块通用优化 / `docs/优化/09` ↔ 拼块 + 玫瑰窗混合。

---

## 4. 阅读约定

- **代码引用格式**：`文件名:行号`，如 `search.rs:839` 表示 `search.rs` 第 839 行。
- **规则 ID**：沿用游戏内部 ID（`precise` / `shape_pool` / `rose_window` …）。
  规则中文名/英文名对照见 03 与 09。
- **ASCII 谜题**：见 [09 附录](09-附录-术语表与puz格式.md) 的 `.puz` 格式说明。
- 本系列**描述的是当前 `main` 分支**的行为；若代码演进，以源码为准。
