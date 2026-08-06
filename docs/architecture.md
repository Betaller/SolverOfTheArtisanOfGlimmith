# 架构设计文档

## 1. 系统概述

TAGSolver 是《格里米斯的工匠》谜题的求解与编辑器系统，采用 Python + PySide6 开发。
系统提供图形化界面用于输入谜题、配置规则，并内置回溯搜索求解器自动求解。

### 1.1 功能边界

| 功能 | 范围 |
|------|------|
| 谜题输入 | 手动绘制网格、标注符号/数字/约束、配置规则 |
| 谜题存储 | JSON 格式序列化/反序列化 |
| 自动求解 | 基于回溯搜索 + 约束传播求解 |
| 结果展示 | 彩色渲染区域划分、标注形状匹配结果 |
| 解法验证 | 独立验证器检查解是否满足所有规则 |

### 1.2 非功能需求

| 需求 | 指标 |
|------|------|
| 网格尺寸 | 支持 2×2 ~ 16×16 |
| 求解耗时 | 8×8 以内 ≤ 30s，10×10 以内 ≤ 5min |
| 内存占用 | ≤ 512MB |
| 平台 | Windows 10+ / macOS / Linux |

---

## 2. 系统架构

### 2.1 分层架构

```
┌──────────────────────────────────────┐
│           UI 层 (PySide6)             │
│  MainWindow / GridWidget / Panels    │
├──────────────────────────────────────┤
│          服务层 (Service)             │
│  PuzzleService / SolverService       │
├──────────────────────────────────────┤
│          求解引擎 (Solver)            │
│  Backtrack / Propagator / Validator  │
├──────────────────────────────────────┤
│       领域模型层 (Models)             │
│  Board / Cell / Edge / Constraint    │
├──────────────────────────────────────┤
│          持久化层 (IO)                │
│  PuzzleSerializer / JsonCodec        │
└──────────────────────────────────────┘
```

### 2.2 模块依赖关系

```
main.py
  └── ui/
        ├── main_window.py       依赖: services.puzzle_service, models
        ├── grid_widget.py       依赖: models
        ├── constraint_panel.py  依赖: models
        └── solver_runner.py     依赖: solver.base（default_router）
  └── services/
        └── puzzle_service.py    依赖: models, io
  └── solver/                    Rust-only 接口 + 规则/形状共享层
        ├── base.py              Solver ABC + SolverRouter（default_router 只挂 RustSolver）
        ├── rust_solver.py       Rust 子进程封装，依赖: models, io
        ├── constraints.py       RULE_CHECKERS（22 条规则校验器），依赖: models
        ├── shapes.py            形状变换/规范化，依赖: models
        └── exceptions.py        求解异常
  └── models/
        ├── board.py
        ├── puzzle.py
        └── solution.py
  └── io/
        └── puzzle_codec.py      依赖: models
```

> 说明：Python 求解算法（backtrack / exact_cover / rose / dlx / propagator 等）已于
> 2026-08-06 移除（`docs/official-puzzles-status.md` §C.0），`src/solver/` 仅保留接口与共享层。
> 求解引擎为 Rust 子进程（`rsolver/`），协议见 `src/solver/rust_solver.py`。

---

## 3. 领域模型

### 3.1 核心数据模型 ER 图

```
Puzzle (1) ──── (N) Rule
   │
   ├── (1) Grid
   │        ├── (N) Cell ──────── (0..1) Symbol
   │        │                      (0..1) ShapePattern
   │        │                      (0..1) Compass
   │        │
   │        ├── (N) Edge ──────── (0..1) EdgeConstraint
   │        │
   │        └── (N) Vertex ────── (0..1) Watchtower
   │
   └── (N) ShapePool ────────── (N) Shape
```

### 3.2 关键类定义

```python
@dataclass
class Cell:
    row: int
    col: int
    region_id: int | None          # None = 未分配

    # 线索
    number: int | None             # 面积规则：数字线索
    shape_pattern: Shape | None    # 拼块规则：形状图案
    compass: Compass | None        # 罗盘规则
    symbols: list[Symbol]          # 符号列表（玫瑰窗/独居）

@dataclass
class Compass:
    up: int                        # -1 表示无限制
    down: int
    left: int
    right: int

@dataclass
class Edge:
    cell1_pos: tuple[int, int]     # (row, col)
    cell2_pos: tuple[int, int]
    is_boundary: bool              # True=分割边框, False=同区域
    constraint: EdgeConstraint | None

@dataclass
class EdgeConstraint:
    type: Literal["heterogeneous", "homogeneous", "inequality", "difference"]
    value: int | None              # 差值规则用

@dataclass
class Vertex:
    row: int                       # 虚拟行坐标（四格交汇点）
    col: int                       # 虚拟列坐标
    watchtower: int | None         # 望塔规则

@dataclass
class Rule:
    type: str                      # 规则编号: "1" ~ "22"
    params: dict                   # 规则参数

@dataclass
class Shape:
    cells: list[tuple[int, int]]   # 相对坐标列表（归一化到原点）

@dataclass
class Puzzle:
    grid_height: int
    grid_width: int
    cells: list[Cell]              # 每个单元格
    edges: list[Edge]              # 内部相邻边
    vertices: list[Vertex]         # 网格绝对角点（含外边界）
    rules: list[Rule]              # 启用的规则列表
    shape_pool: list[Shape]        # 形状池

@dataclass
class Solution:
    puzzle: Puzzle
    region_assignments: dict[int, list[Cell]]  # region_id -> cells
    region_shapes: dict[int, Shape]            # region_id -> normalized shape
    solved: bool
    steps_taken: int
    elapsed_ms: int
```

### 3.3 棋盘建模约定

```
单元格坐标: (row, col)  0-indexed
  row: 0 ~ H-1
  col: 0 ~ W-1

边框 (Edge):
  - 水平边框: (r, c) 与 (r, c+1) 之间, 范围 r∈[0,H-1], c∈[0,W-2]
  - 垂直边框: (r, c) 与 (r+1, c) 之间, 范围 r∈[0,H-2], c∈[0,W-1]

顶点 (Vertex):
  - 网格角点: (r, c) 表示网格绝对角点 (r∈[0,H], c∈[0,W], **含外边界**)
  - 角点 (r,c) 接触的单元格 = 在界的 {(r-1,c-1),(r-1,c),(r,c-1),(r,c)}（内部 4 格、边 2 格、角 1 格）
  - (2026-08-06 变更：原为「四格交汇点 (vr,vc)，范围 vr∈[0,H-1]」，无法表示边界望塔)
```

---

## 4. 求解引擎设计

### 4.1 求解流程

```
输入: Puzzle (网格 + 规则列表)
  │
  ▼
预处理阶段
  ├── 构建 Cell 邻接关系
  ├── 提取规则依赖图（确定约束传播顺序）
  ├── 识别单值约束（如特定单元格必须独立成区）
  └── 初始化域（每个格子的候选颜色集合）
  │
  ▼
回溯搜索 (DFS)
  ├── 选变量：MRV (最少剩余值) 启发式
  ├── 赋值：尝试候选颜色
  ├── 前向检查：传播约束，修剪邻接域
  ├── 一致性检查：
  │   ├── 连通性检查（同色区域必须四连通）
  │   ├── 局部约束（面积数字、拼块、罗盘等）
  │   └── 全局约束（形状池、相同/相异等，区域完成后检查）
  ├── 成功 → 继续下一个未赋值格
  └── 失败 → 回溯
  │
  ▼
后处理阶段
  ├── 形状归一化与匹配验证
  ├── 全局约束最终验证
  └── 输出 Solution
```

### 4.2 约束传播优先级

约束按影响范围和计算代价分层：

| 层级 | 约束 | 传播时机 |
|------|------|----------|
| L0 拓扑 | 连通性、区域划分 | 每次赋值后 |
| L1 局部数值 | 面积数字、范围、精确、罗盘、不等号、差值 | L0 稳定后 |
| L1 局部符号 | 玫瑰窗、独居 | L0 稳定后 |
| L2 边界约束 | 异生、双生、围栏 | L1 满足后 |
| L3 局部形状 | 拼块 | L2 满足后 |
| L4 全局形状 | 形状池、相同、相异、混合 | 完整区域形成后 |
| L4 全局拓扑 | 方块、非方块、砖纹、环纹、差异化 | 完整区域形成后 |
| L5 全局顶点 | 望塔 | 所有区域确定后 |

### 4.3 形状匹配引擎

```
输入: 区域 A (一组单元格坐标)
  │
  ▼
1. 提取形状位图
   ├── 计算最小包围盒 (bbox)
   ├── 生成 H×W 位图矩阵
   └── 对齐到原点 (0,0)
  │
  ▼
2. 生成规范形
   ├── 旋转 0° / 90° / 180° / 270°
   ├── 水平翻转 + 各旋转
   ├── 垂直翻转 + 各旋转
   └── 共 8 种变换 → 取字典序最小的位图作为规范形
  │
  ▼
3. 形状匹配
   ├── 与形状池比对 (规范形哈希)
   ├── 与另一区域比对 (规范形相等)
   └── 与拼块线索比对 (规范形相等)
  │
  ▼
输出: ShapeMatchResult { normalized, matched_pool, matched_partner }
```

### 4.4 回溯搜索性能优化

| 策略 | 说明 |
|------|------|
| MRV 启发式 | 优先选择候选颜色最少的格子 |
| 度启发式 (tie-breaker) | MRV 相同时选约束度最高的格子 |
| 前向检查 (Forward Checking) | 赋值后立即剪枝相邻格的候选域 |
| AC-3 | 对候选域进行弧一致性维护 |
| 区域生长剪枝 | 若某区域大小已超出面积上限，提前回溯 |
| 孤岛检测 | 若未分配区域形成孤岛且面积不符，提前回溯 |
| 对称性破缺 | 颜色标签交换对称，固定第一个区域的编号 |
| 解缓存 | 对已探索的状态缓存结果（带截止大小） |

---

## 5. UI 设计

### 5.1 界面布局

```
┌──────────────────────────────────────────────────┐
│  菜单栏 (文件 / 编辑 / 求解 / 帮助)               │
├─────────────────────┬────────────────────────────┤
│                     │                            │
│  工具箱              │   编辑区 / 展示区           │
│  ┌───────────────┐  │   ┌────────────────────┐   │
│  │ 选择工具       │  │   │                    │   │
│  │ 边框绘制       │  │   │    网格画布         │   │
│  │ 颜色填充       │  │   │    (缩放/平移)      │   │
│  │ 符号工具       │  │   │                    │   │
│  │ 数字工具       │  │   └────────────────────┘   │
│  │ 罗盘工具       │  │                            │
│  │ 拼块工具       │  │                            │
│  │ 望塔工具       │  │                            │
│  │ 约束工具       │  │                            │
│  └───────────────┘  │                            │
│                     │                            │
│  规则面板            │                            │
│  ┌───────────────┐  │                            │
│  │ ☑ 形状池      │  │                            │
│  │ ☑ 玫瑰窗      │  │                            │
│  │ ☑ 异生        │  │                            │
│  │ ...           │  │                            │
│  │ ☑ 罗盘        │  │                            │
│  └───────────────┘  │                            │
│                     │                            │
│  属性面板            │                            │
│  ┌───────────────┐  │                            │
│  │ 选中对象属性   │  │                            │
│  │ 单元格/边框/   │  │                            │
│  │ 顶点 属性编辑  │  │                            │
│  └───────────────┘  │                            │
├─────────────────────┴────────────────────────────┤
│  状态栏 (坐标 / 操作提示 / 求解进度)               │
└──────────────────────────────────────────────────┘
```

### 5.2 交互模式

| 模式 | 操作 | 反馈 |
|------|------|------|
| 选择 | 点击/框选单元格 | 高亮选中 |
| 绘制边框 | 点击两相邻格之间边 | 切换分割/连通 |
| 填充颜色 | 选择颜色后点击格 | 填充选中格 |
| 标注符号 | 选择符号后点击格 | 符号图标显示 |
| 标注数字 | 输入数字后点击格 | 数字显示 |
| 罗盘 | 选择后依次设 4 方向 | 方向箭头显示 |
| 约束 | 点击边框标记不等号/差值 | 约束图标显示 |
| 望塔 | 点击顶点 | 数字显示在顶点 |
| 拼块 | 选择形状后点击格 | 形状缩略图 |
| 形状池编辑 | 在弹出编辑器中添加形状 | 形状列表更新 |

### 5.3 求解展示

求解过程在网格画布上实时渲染：

| 状态 | 视觉表现 |
|------|----------|
| 未分配 | 灰色，虚线网格 |
| 已分配 | 填充区域颜色，同区域同色 |
| 分割边框 | 粗实线 |
| 连通边 | 无边框（区域内部） |
| 错误冲突 | 红色闪烁高亮 |
| 最终解 | 区域颜色 + 形状标签 + 验证通过绿框 |

### 5.4 数据流

```
用户操作 → GridWidget (事件) → PuzzleService (更新模型)
                                      │
                              SolverService (调用求解器)
                                      │
                              Solution → GridWidget (渲染)
                                      │
                              Validator → 结果展示
```

---

## 6. 持久化设计

### 6.1 JSON 格式

```json
{
  "version": "1.0",
  "grid": {
    "height": 6,
    "width": 6
  },
  "cells": [
    {
      "row": 0, "col": 0,
      "number": 3,
      "symbol": null,
      "shape_pattern": null,
      "compass": null
    }
  ],
  "edges": [
    {
      "r1": 0, "c1": 0, "r2": 0, "c2": 1,
      "constraint": null
    }
  ],
  "vertices": [
    {
      "row": 0, "col": 0,
      "watchtower": null
    }
  ],
  "rules": [
    {"type": "shape_pool", "params": {"shapes": ["L", "I", "O"]}},
    {"type": "area", "params": {}},
    {"type": "range", "params": {"min": 2, "max": 5}},
    {"type": "precise", "params": {"area": 4}}
  ],
  "shape_pool": [
    {"id": "L", "cells": [[0,0],[1,0],[1,1]]},
    {"id": "I", "cells": [[0,0],[0,1],[0,2]]},
    {"id": "O", "cells": [[0,0],[0,1],[1,0],[1,1]]}
  ]
}
```

### 6.2 文件组织

```
puzzles/
├── tutorials/          # 教学关
├── official/           # 官方关卡
├── user/               # 用户自建关卡
└── solutions/          # 已求解的存档
```

---

## 7. 错误处理

| 场景 | 处理方式 |
|------|----------|
| 非法网格尺寸 | 提示范围限制，拒绝创建 |
| 规则冲突 | 求解器返回不可解 + 冲突规则分析 |
| 无解 | 提示无解，显示搜索终止时的最优部分解 |
| UI 输入错误 | 即时校验 + 错误状态提示 |
| 文件格式错误 | 友好错误提示 + 定位问题行 |
| 求解超时 | 终止搜索，报告当前进度 |
| 内存超限 | 限制搜索深度/缓存大小，触发 GC |
