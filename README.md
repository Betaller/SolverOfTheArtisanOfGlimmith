# SolverOfTheArtisanOfGlimmith

《格里米斯的工匠》(The Artisan of Glimmith) 谜题求解器。

## Solved：一个数独、数织、数桥... 加强版的区域分割谜题

谜题的核心目标：将矩形网格划分为多个连通区域，使得所有区域内外的线索条件得到满足。

## 运行

```powershell
python src/app.py                      # Qt UI
python -m pytest tests/ -x --tb=short  # 365 个测试
python scripts/verify_puzzles.py       # 验证所有谜题 (20s 超时)
```

## 多求解器架构

```
src/solver/
├── base.py               # Solver ABC + SolverRouter 路由层
├── backtrack.py          # 通用回溯求解器 (默认)
├── dlx.py                # 舞蹈链 (Dancing Links / Algorithm X)
├── exact_cover/
│   └── solver.py         # 精确覆盖求解器 — 形状池 / 固定面积 / 块形
├── rose/
│   └── solver.py         # 玫瑰窗专用求解器 — 区域匹配 + BFS 增长
├── constraints.py        # 22 条规则校验器
├── candidates.py         # 候选区域生成 / 枚举
├── checks.py             # 增量 / 全局校验
├── shapes.py             # 多联骨牌变换、规范化
├── region_match.py       # 区域匹配 (玫瑰窗)
├── rose_growth.py        # BFS 增长 + 交换修复 (玫瑰窗)
├── bfs_candidates.py     # BFS 候选生成
├── validator.py          # 最终解验证
└── propagator.py         # 约束传播 / 边界更新
```

### 路由策略

```
谜题 → ExactCoverSolver → RoseSolver → BacktrackSolver
        精确覆盖           玫瑰窗       通用回溯 (兜底)
```

| 求解器 | 适用场景 | 算法 |
|--------|---------|------|
| **ExactCoverSolver** | shape_pool / block+precise / precise小网格 | DLX 舞蹈链 + 形状变换枚举 |
| **RoseSolver** | rose_window 无尺寸约束 | 区域匹配 (MRV) → BFS 增长 + 交换修复 |
| **BacktrackSolver** | 万能兜底 | 逐区域 DFS + 多级剪枝 + 组件可行性分析 |

## 规则实现状态

全部 22 条规则均已实现，绿色 = 对应求解器完全支持。

| 规则 | 中文 | 类型 | 支持 |
|------|------|------|:--:|
| shape_pool | 形状池 | 全局 | ✅ 精确覆盖 + 回溯 |
| rose_window | 玫瑰窗 | 单元格 | ✅ 专用求解器 + 回溯 |
| heterogeneous | 异生 | 边 | ✅ |
| homogeneous | 双生 | 边 | ✅ |
| precise | 精确面积 | 全局 | ✅ 精确覆盖 + 回溯 |
| puzzle_piece | 拼块 | 单元格 | ✅ |
| mixed | 混合 | 全局 | ✅ |
| area | 面积数字 | 单元格 | ✅ |
| same | 相同形状 | 全局 | ✅ |
| range | 面积范围 | 全局 | ✅ |
| fence | 围栏 | 单元格 | ✅ |
| different | 相异形状 | 全局 | ✅ |
| solitary | 独居 | 全局 | ✅ |
| block | 方块 (矩形) | 全局 | ✅ 精确覆盖 + 回溯 |
| non_block | 非方块 | 全局 | ✅ |
| differentiation | 差异化 | 全局 | ✅ |
| brick | 砖纹 (禁十字) | 全局 | ✅ |
| ring | 环纹 (禁T字) | 全局 | ✅ |
| inequality | 不等号 | 边 | ✅ 弧一致性传播 |
| difference | 差值 | 边 | ✅ 弧一致性传播 |
| watchtower | 望塔 | 顶点 | ✅ |
| compass | 罗盘 | 单元格 | ✅ |

### 已知限制

| 限制 | 说明 |
|------|------|
| 网格尺寸 | 2×2 ~ 16×16 |
| 罗盘 + 无尺寸约束 | 若没有 precise/range/shape_pool 限定区域大小，罗盘谜题搜索空间大 |
| 独居 + 无尺寸约束 | 同上，需要先生成全部候选再精确覆盖 |
| 超大网格精确覆盖 | 11×11 以上形状池候选数可能超 10 万，超时 |
| 预定义分割线 + 玫瑰窗 | C4-2 等复杂预切玫瑰窗谜题耗时较长 |

## 官方谜题求解状态与文档软门禁

官方题库（`puzzles/official/`，1231 题）的求解进度、DIFF/UNSOLVED 分析、根因结论与后续计划，
统一维护在 **`docs/official-puzzles-status.md`**。要点：

- 官方题官方解**唯一**是准则；历史上的「求解器解≠官方解」绝大多数是转换/校验 bug，已修复
  （gemini/delta 边约束、玫瑰窗检测、环纹边框 T 型、1SPR shape 约束）。
- 当前全量扫描：Rust-only 基准 1052/1258 通过（较上基准 +5，两个 brick 回溯缺口 1301/0957 已闭合），
  6 道 watchtower 待甄别，详细数字见该文档。
- **软门禁**：对求解器 / 转换 / 校验 / 规则语义的每次优化，合入前必须更新该文档及相关文档、
  跑通测试，否则视为未完成（详见 `docs/official-puzzles-status.md` §6 与 `CLAUDE.md`）。

## 验证结果

```
谜题来源        数量   通过   说明
─────────────────────────────────────
官方/自定义      42    38    离线自然/社区谜题
参考 (aog 转换)  22    10    多样规则组合测试

总计            64    48    (75%)
```

## C++ AoG 官方谜题库 (`aog_puzzles/`)

`aog_puzzles/` 存放从官方存档 `third_party/archiveofglimmith.github.io/puzzles.json`
生成的 **C++ AoG_Solver `.puz` 格式**谜题（1231 个，`aog_puzzles/<zone>/<type>/<id>.puz`），
可直接用于参考求解器 `third_party/AoG_Solver` 的批量验证：

```bash
python scripts/convert_puzzles_json_to_aog.py   # 重新生成到 aog_puzzles/
cd third_party/AoG_Solver && ./batch_run.sh ../aog_puzzles/Zone1   # 批量求解验证
```

**生成逻辑**：archive 的 `puzzle_grid` / `solution` 本身就是游戏原生的 .puz 网格，只是每行
尾随空格被裁剪。转换器逐行补齐到 C++ 解析器所需的宽度：

- 节点行补齐到 `3*width+1` 字符；
- 区域行按解析器 `size` 增长规则补齐（罗盘 `U...` 单元格会撑宽行，裁剪的行会导致越界
  读取甚至段错误，旧脚本的 6-compass 谜题即因此崩溃）；
- `SHAPE` 每行补齐到最大宽度（C++ 按「最后一行长度」取尺寸，行宽不均会丢格子）。

**验证工作流**：

```bash
python scripts/compare_batch_ansi.py --ref third_party/AoG_Solver/Zone1.ansi \
    --new <batch_run输出>     # 对比谜题路径与状态 (correct/timeout/...)
python scripts/fix_puz_solutions.py --zone Zone1 --batch <batch_run输出> \
    --root aog_puzzles        # 固化缺失/多解谜题的 SOLUTION (如 0067)
```

验证结果与官方 batch 日志对比：**Zone1 312/312、Zone2 438/438、Zone3 479-481/481**
完全一致（残余差异均为 10s 超时边界的机器计时抖动，谜题解均与官方解一致）。

## 参考项目

| 仓库 | 语言 | 借鉴 |
|------|------|------|
| Neptune17/AoG_Solver | C++ | 多级种子选择、组件可行性剪枝 |
| lifthrasiir/aog | Rust | DLX 舞蹈链、不等式弧一致性、多求解器架构 |
| shartiniquais/glimmith-solver | JS | 精确覆盖 + 候选过滤模式 |
| hhhxiao/TAGSolver | Python | 相同形状并行增长 |
| acasperw/shape-helper | TS | 形状可视化 |

## 开发

```powershell
ruff check src/ tests/    # lint
ruff format src/ tests/   # format
mypy src/                 # typecheck
```
