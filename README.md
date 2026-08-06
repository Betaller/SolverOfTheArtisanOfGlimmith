# SolverOfTheArtisanOfGlimmith

《格里米斯的工匠》(The Artisan of Glimmith) 谜题求解器与编辑器。

谜题核心目标：把矩形网格划分为多个连通区域，使得**所有区域内外的线索条件**（单元格 / 边 / 顶点 / 全局）都得到满足。共 **22 条规则**（见 [规则支持](#规则支持)）。

## 快速开始

```bash
cd rsolver && cargo build --release   # 构建 Rust 求解器（默认路由必需）
python src/app.py                     # Qt UI (PySide6)
python -m pytest tests/ -x --tb=short # 全量测试 (~290 个)
python scripts/verify_puzzles.py      # 验证全部谜题 (每题 30s 超时)
python scripts/benchmark_rust_solver.py  # 官方题基准（每题 20s 超时）
```

> `verify_puzzles.py` / `benchmark_rust_solver.py` 会对每个解出的**官方题**额外比对
> `*-answer` 目录的**官方唯一解**：解合法但与官方解分区不同会标记 `DIFF` 并计入失败
> （官方解唯一性准则）。非官方谜题（reference / user / aiGen）无官方解，跳过该比对。

> `default_router()` 会急切构造 `RustSolver`，因此运行 app 与 `verify_puzzles.py` 之前必须先 `cargo build --release`（二进制在 `rsolver/target/release/rsolver`）。

## 求解器架构

**Rust 求解器**（`rsolver/`）是唯一求解引擎。Python 侧（`src/solver/`）只保留路由接口与
规则/形状共享层——曾经的 Python 求解算法（精确覆盖 / 玫瑰窗 / 回溯）已在 2026-08-06 移除
（全语料评估证明它们解不出 Rust 引擎解不出的题，见 `docs/official-puzzles-status.md` §C.0）。

### 路由策略（Python 侧）

`SolverRouter`（`src/solver/base.py`）当前只挂 RustSolver：

```
RustSolver
```

**关键不变量**：路由器对求解器的答案都经 `IndependentValidator`
（`src/validation/validator.py`）独立重验证，与求解器内部规则检查解耦。答案错误会被拦截并
记为失败——任何求解器 bug 都不可能把错误解"走私"出来。

### Rust 求解器（`rsolver/`）

子进程协议：谜题 JSON → stdin，题解 JSON → stdout（`rsolver --batch` 支持多行输入逐行输出，
`RustSolver.solve_batch` 复用一个子进程批量求解）。内部按顺序尝试 4 个算法，先出答案者胜：

```
① AoG DFS（主力，C++ 参考求解器的 1:1 移植 + 剪枝）→ ② Rose（纯玫瑰窗）
→ ③ Pieces/DLX（形状池 / 面积 / 罗盘 → 精确覆盖）→ ④ Backtrack（逐区域 DFS，兜底）
```

aog 求解器内部检查视为权威（`build_solution_trusted`），Rust 侧不再重验证；rose/pieces/
backtrack 的答案须过 `solver/validate.rs` 验收门。

### 目录结构

```
src/solver/               Python 接口与共享层
├── base.py                Solver ABC + SolverRouter 路由层（Rust-only）
├── rust_solver.py         Rust 子进程封装（block→形状池转换等）
├── constraints.py         22 条规则校验器（UI / 校验共用）
├── shapes.py              多联骨牌变换、规范化（UI / 脚本共用）
└── exceptions.py          求解异常

rsolver/src/               Rust 求解器
├── main.rs / mod.rs       入口 + 路由调度
├── solver/
│   ├── aog/               AoG DFS（C++ 移植）
│   ├── rose/              玫瑰窗（cells / region_match / rose_growth）
│   ├── pieces.rs + dlx.rs 精确覆盖
│   └── backtrack.rs       区域回溯
└── constraints.rs / types.rs / grid.rs / polyomino.rs
```

## 规则支持

全部 22 条规则均已实现。

| 规则 | 中文 | 类型 | 支持 |
|------|------|------|:--:|
| shape_pool | 形状池 | 全局 | ✅ 精确覆盖 + 回溯 |
| rose_window | 玫瑰窗 | 单元格 | ✅ 专用求解器 + 回溯 |
| heterogeneous | 异生 | 边 | ✅ |
| homogeneous | 双生 | 边 | ✅ |
| precise | 精确 | 全局 | ✅ 精确覆盖 + 回溯 |
| puzzle_piece | 拼块 | 单元格 | ✅ |
| mixed | 混合 | 全局 | ✅ |
| area | 面积 | 单元格 | ✅ |
| same | 相同 | 全局 | ✅ |
| range | 范围 | 全局 | ✅ |
| fence | 围栏 | 单元格 | ✅ |
| different | 相异 | 全局 | ✅ |
| solitary | 独居 | 全局 | ✅ |
| block | 方块 | 全局 | ✅ 精确覆盖 + 回溯 |
| non_block | 非方块 | 全局 | ✅ |
| differentiation | 差异化 | 全局 | ✅ |
| brick | 砖纹 | 全局 | ✅ |
| ring | 环纹 | 全局 | ✅ |
| inequality | 不等号 | 边 | ✅ 弧一致性传播 |
| difference | 差值 | 边 | ✅ 弧一致性传播 |
| watchtower | 望塔 | 顶点 | ✅ |
| compass | 罗盘 | 单元格 | ✅ |

每条规则在 Rust 各求解器中的具体实现见 `docs/rust-solver/03-规则与代码映射.md`。

### 已知限制

| 限制 | 说明 |
|------|------|
| 网格尺寸 | 2×2 ~ 16×16 |
| 罗盘 + 无尺寸约束 | 没有 precise/range/shape_pool 限定区域大小时，搜索空间大 |
| 独居 + 无尺寸约束 | 同上，需先枚举全部候选再精确覆盖 |
| 超大网格精确覆盖 | 11×11 以上形状池候选数可能超 10 万，超时 |
| 复杂组合 | compass / rose / ring 强规则组合剪枝不足，仍有个别题解不出 |
| 预定义分割线 + 玫瑰窗 | 复杂预切玫瑰窗题耗时较长 |

## 官方题库与求解状态

官方题库（`puzzles/official/`，**1258 题**）的求解进度、DIFF / UNSOLVED 分析、根因结论与
后续计划，统一维护在 **`docs/official-puzzles-status.md`**。要点：

- 官方解是**唯一解**是准则；历史上的「求解器解 ≠ 官方解」绝大多数是转换/校验 bug，已修复
  （gemini/delta 边约束、玫瑰窗检测、环纹边框 T 型、brick/形状规则语义等）。
- 当前 Rust-only 全量基准 **1052/1258 通过**（最新 commit `dfadfe3`），详细数字与 zone 分布
  见该文档；6 道 watchtower DIFF 待甄别。
- **软门禁**：对求解器 / 转换 / 校验 / 规则语义的每次优化，提交前必须更新该文档（进度 +
  变更各追加一条）、同步相关文档、跑通测试，并把基准结果随提交入库
  （`results/YYYYMMDD_<short-sha>.txt`）。详见该文档附录 D 与 `CLAUDE.md`。

## C++ AoG 官方谜题库 (`aog_puzzles/`)

`aog_puzzles/` 存放从官方存档 `third_party/archiveofglimmith.github.io/puzzles.json`
生成的 **C++ AoG_Solver `.puz` 格式**谜题（**1231 个**，`aog_puzzles/<zone>/<type>/<id>.puz`），
可直接用于参考求解器 `third_party/AoG_Solver` 的批量验证：

```bash
python scripts/convert_puzzles_json_to_aog.py   # 重新生成到 aog_puzzles/
cd third_party/AoG_Solver && ./batch_run.sh ../aog_puzzles/Zone1   # 批量求解验证
python scripts/compare_batch_ansi.py --ref third_party/AoG_Solver/Zone1.ansi \
    --new <batch_run输出>     # 对比谜题路径与状态 (correct/timeout/...)
```

**生成逻辑**：archive 的 `puzzle_grid` / `solution` 本身就是游戏原生的 .puz 网格，只是每行
尾随空格被裁剪。转换器逐行补齐到 C++ 解析器所需的宽度：节点行补到 `3*width+1` 字符；区域行
按解析器 `size` 增长规则补齐（罗盘 `U...` 单元格会撑宽行，裁剪会导致越界读取甚至段错误）；
`SHAPE` 每行补齐到最大宽度（C++ 按「最后一行长度」取尺寸，行宽不均会丢格子）。

验证结果与官方 batch 日志一致：**Zone1 312/312、Zone2 438/438、Zone3 479-481/481**
（残余差异均为 10s 超时边界的机器计时抖动，谜题解均与官方解一致）。

## 参考项目

| 仓库 | 语言 | 借鉴 |
|------|------|------|
| Neptune17/AoG_Solver | C++ | 多级种子选择、组件可行性剪枝 |
| lifthrasiir/aog | Rust | DLX 舞蹈链、不等式弧一致性、多求解器架构 |
| shartiniquais/glimmith-solver | JS | 精确覆盖 + 候选过滤模式 |
| hhhxiao/TAGSolver | Python | 相同形状并行增长 |
| acasperw/shape-helper | TS | 形状可视化 |

## 文档索引

| 文档 | 内容 |
|------|------|
| `docs/architecture.md` | Python 侧架构设计 |
| `docs/rules-guide.md` | 22 条规则详解与解题技巧 |
| `docs/official-puzzles-status.md` | 官方题库求解进度 / DIFF / UNSOLVED / 软门禁 |
| `docs/rust-solver/README.md` | Rust 求解器源码详解系列（01-09，含算法流程图、数据结构图、规则映射表） |
| `docs/testing.md` / `docs/faq.md` | 测试指南 / 常见问题 |
| `CLAUDE.md` | 对 Claude Code 的仓库指引（含文档软门禁） |

## 开发

```bash
ruff check src/ tests/    # lint (line-length=100)
ruff format src/ tests/   # format
mypy src/                 # typecheck (strict)
cd rsolver && cargo test  # Rust 测试
```
