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

## 验证结果

```
谜题来源        数量   通过   说明
─────────────────────────────────────
官方/自定义      42    38    离线自然/社区谜题
参考 (aog 转换)  22    10    多样规则组合测试

总计            64    48    (75%)
```

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
