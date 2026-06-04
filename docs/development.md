# 开发指南

## 1. 环境要求

### 1.1 系统要求

| 项目 | 要求 |
|------|------|
| Python | ≥ 3.10 (推荐 3.12) |
| 操作系统 | Windows 10+ / macOS 12+ / Linux (GNOME/KDE) |
| 显示 | 分辨率 ≥ 1280×720，支持 OpenGL 2.0+ |
| 磁盘 | ≥ 500MB 可用空间 |
| 内存 | ≥ 2GB |

### 1.2 依赖清单

```
PySide6>=6.6.0           # Qt 绑定，UI 框架
numpy>=1.24.0            # 矩阵运算（形状匹配）
attrs>=23.1.0            # 数据类增强
json5>=0.9.14            # 带注释的 JSON 解析
pytest>=8.0.0            # 单元测试
pytest-benchmark>=4.0.0  # 性能基准测试
coverage>=7.0.0          # 测试覆盖率
black>=24.0.0            # 代码格式化
ruff>=0.3.0              # 代码检查
mypy>=1.8.0              # 类型检查
pre-commit>=3.6.0        # 提交前检查
```

### 1.3 安装步骤

```powershell
# 1. 克隆仓库
git clone <repo-url>
cd TAGSolver

# 2. 创建虚拟环境
python -m venv .venv

# 3. 激活虚拟环境
# Windows:
.venv\Scripts\Activate.ps1
# macOS/Linux:
# source .venv/bin/activate

# 4. 安装依赖
pip install -r requirements.txt
# 或使用 uv（推荐，更快）：
# pip install uv && uv pip install -r requirements.txt

# 5. 安装开发依赖
pip install -r requirements-dev.txt

# 6. 安装 pre-commit hooks
pre-commit install
```

---

## 2. 项目结构

```
TAGSolver/
├── .venv/                    # 虚拟环境 (gitignored)
├── .opencode/                # OpenCode 配置 (gitignored)
│
├── docs/                     # 文档
│   ├── architecture.md       # 架构设计
│   ├── development.md        # 开发指南（本文件）
│   ├── acceptance.md         # 验收标准
│   └── testing.md            # 测试计划
│
├── src/
│   ├── __init__.py
│   │
│   ├── models/               # 领域模型
│   │   ├── __init__.py
│   │   ├── board.py          # Cell, Edge, Vertex
│   │   ├── puzzle.py         # Puzzle, Rule, Shape
│   │   └── solution.py       # Solution
│   │
│   ├── solver/               # 求解引擎
│   │   ├── __init__.py
│   │   ├── shapes.py         # 形状匹配引擎
│   │   ├── constraints.py    # 约束定义
│   │   ├── propagator.py     # 约束传播
│   │   ├── backtrack.py      # 回溯搜索
│   │   ├── validator.py      # 解验证器
│   │   └── exceptions.py     # 求解异常
│   │
│   ├── services/             # 服务层
│   │   ├── __init__.py
│   │   ├── puzzle_service.py # 谜题 CRUD 服务
│   │   └── solver_service.py # 求解任务管理
│   │
│   ├── ui/                   # UI 层
│   │   ├── __init__.py
│   │   ├── main_window.py    # 主窗口
│   │   ├── grid_widget.py    # 网格画布组件
│   │   ├── constraint_panel.py # 规则配置面板
│   │   ├── tool_palette.py   # 工具箱面板
│   │   ├── property_panel.py # 属性面板
│   │   ├── shape_editor.py   # 形状编辑器弹窗
│   │   └── solver_runner.py  # 求解器集成
│   │
│   ├── io/                   # 持久化
│   │   ├── __init__.py
│   │   ├── puzzle_codec.py   # JSON 编解码
│   │   └── file_manager.py   # 文件管理
│   │
│   └── app.py                # 应用入口
│
├── tests/                    # 测试
│   ├── __init__.py
│   ├── conftest.py           # pytest fixtures
│   ├── unit/
│   │   ├── test_board.py
│   │   ├── test_shapes.py
│   │   ├── test_constraints.py
│   │   ├── test_backtrack.py
│   │   ├── test_validator.py
│   │   ├── test_propagator.py
│   │   ├── test_puzzle_codec.py
│   │   └── test_rules/
│   │       ├── test_rule_01_shape_pool.py
│   │       ├── test_rule_02_rose_window.py
│   │       ├── test_rule_03_heterogeneous.py
│   │       ├── test_rule_04_homogeneous.py
│   │       ├── test_rule_05_precise.py
│   │       ├── test_rule_06_puzzle_piece.py
│   │       ├── test_rule_07_mixed.py
│   │       ├── test_rule_08_area.py
│   │       ├── test_rule_09_same.py
│   │       ├── test_rule_10_range.py
│   │       ├── test_rule_11_fence.py
│   │       ├── test_rule_12_different.py
│   │       ├── test_rule_13_solitary.py
│   │       ├── test_rule_14_block.py
│   │       ├── test_rule_15_non_block.py
│   │       ├── test_rule_16_differentiation.py
│   │       ├── test_rule_17_brick.py
│   │       ├── test_rule_18_ring.py
│   │       ├── test_rule_19_inequality.py
│   │       ├── test_rule_20_difference.py
│   │       ├── test_rule_21_watchtower.py
│   │       └── test_rule_22_compass.py
│   ├── integration/
│   │   ├── test_solver_end_to_end.py
│   │   └── test_rule_combinations.py
│   └── system/
│       ├── test_full_puzzles.py
│       ├── test_performance.py
│       └── test_ui_basic.py
│
├── puzzles/                  # 谜题仓库
│   ├── tutorials/            # 教学关（单个规则）
│   ├── official/             # 官方关卡组合
│   └── user/                 # 用户自建
│
├── scripts/                  # 工具脚本
│   ├── generate_puzzles.py   # 随机谜题生成器
│   └── benchmark.py          # 性能基准
│
├── pyproject.toml            # 项目元数据 + 工具配置
├── requirements.txt          # 生产依赖
├── requirements-dev.txt      # 开发依赖
├── pre-commit-config.yaml    # pre-commit 配置
├── .gitignore
├── .editorconfig
├── LICENSE
└── README.md
```

---

## 3. 开发流程

### 3.1 工作分支规范

```
main          # 稳定版，只合入经过 review 的 feature branch
├── dev       # 开发主线
├── feat/XXX  # 功能分支
├── fix/XXX   # 修复分支
└── doc/XXX   # 文档分支
```

### 3.2 提交规范

使用 [Conventional Commits](https://www.conventionalcommits.org/)：

```
<type>(<scope>): <description>

[optional body]
```

| type | 用途 |
|------|------|
| feat | 新功能 |
| fix | 修复 |
| docs | 文档 |
| test | 测试 |
| refactor | 重构 |
| perf | 性能 |
| style | 格式 |

示例：
```
feat(solver): 实现 AC-3 约束传播引擎
test(rule-08): 添加面积规则测试用例
fix(ui): 修复望塔工具高亮不刷新问题
```

### 3.3 代码审查要求

每个 PR 合并前需满足：
- [ ] 所有测试通过 (`pytest`)
- [ ] 类型检查通过 (`mypy src/`)
- [ ] 代码风格检查通过 (`ruff check src/`)
- [ ] 覆盖率不下降 (`coverage run -m pytest && coverage report`)
- [ ] 至少 1 名 reviewer 批准

### 3.4 CI/CD 流水线

```
阶段 1: Lint & Type Check
  ├── ruff check src/
  └── mypy src/

阶段 2: Unit Tests
  ├── pytest tests/unit/ -x -q
  └── coverage report --fail-under=80

阶段 3: Integration Tests
  └── pytest tests/integration/ -x -q

阶段 4: System Tests
  └── pytest tests/system/ -x -q

阶段 5: Build
  └── pyinstaller --onefile src/app.py
```

---

## 4. 编码规范

### 4.1 Python 约定

| 规范 | 标准 |
|------|------|
| Python 版本 | 3.10+ (使用 `from __future__ import annotations`) |
| 缩进 | 4 空格，无 Tab |
| 行宽 | 100 字符 |
| 引号 | 双引号 |
| 类型标注 | 全员标注，`mypy --strict` 通过 |
| 命名 | `snake_case` 变量/函数, `PascalCase` 类, `UPPER_CASE` 常量 |
| 空行 | 2 空行在类/函数定义之间，1 空行在方法之间 |
| 魔法方法 | `__all__` 显式导出 |
| 异常 | 自定义异常继承 `PuzzleError(BaseException)` |

### 4.2 文档字符串

```python
def solve(puzzle: Puzzle, timeout: int = 30) -> Solution:
    """求解谜题。

    使用回溯搜索 + 约束传播引擎求解给定谜题。
    支持超时中断。

    Args:
        puzzle: 待求解的谜题对象。
        timeout: 超时时间（秒），默认 30s。

    Returns:
        求解结果，包含完整的区域分配。

    Raises:
        SolverTimeoutError: 求解超时。
        NoSolutionError: 无解。

    Example:
        >>> sol = solve(puzzle, timeout=60)
        >>> sol.solved
        True
    """
```

### 4.3 工具配置

**pyproject.toml:**
```toml
[project]
name = "tagsolver"
version = "0.1.0"
requires-python = ">=3.10"

[tool.ruff]
line-length = 100
target-version = "py310"
select = ["E", "F", "W", "I", "N", "UP", "B", "SIM"]

[tool.mypy]
strict = true
python_version = "3.10"
disallow_untyped_defs = true
disallow_any_unimported = true
warn_unused_ignores = true

[tool.pytest.ini_options]
testpaths = ["tests"]
markers = [
    "slow: 耗时较长的测试",
    "solver: 求解器相关测试",
    "ui: UI 相关测试",
]
```

---

## 5. 构建与发布

### 5.1 开发运行

```powershell
# 开发模式（热重载）
python src/app.py

# 或带调试参数
python src/app.py --debug --puzzle puzzles/tutorials/area_01.json
```

### 5.2 打包

```powershell
# 使用 PyInstaller 打包成单文件
pyinstaller --onefile --windowed --name "TAGSolver" src/app.py

# 输出在 dist/TAGSolver.exe
```

### 5.3 版本号

遵循语义化版本 `MAJOR.MINOR.PATCH`：

| 版本 | 说明 |
|------|------|
| 0.1.0 | 核心模型 + 形状匹配 |
| 0.2.0 | 回溯搜索 + 基础规则 |
| 0.3.0 | 全部 22 条规则 |
| 0.4.0 | PySide6 UI 原型 |
| 1.0.0 | 正式发布 |

---

## 6. 性能分析

### 6.1 Profiling

```powershell
# 使用 cProfile 分析求解性能
python -m cProfile -o profile.stats src/app.py --solve puzzles/hard.json
python -m pstats profile.stats

# 使用 Py-Spy（采样分析器）
pip install py-spy
py-spy record -o profile.svg -- python src/app.py --solve puzzles/hard.json
```

### 6.2 调优目标

| 网格 | 规则数 | 目标耗时 |
|------|--------|----------|
| 4×4 | 1-2 | ≤ 1s |
| 6×6 | 2-4 | ≤ 5s |
| 8×8 | 4-6 | ≤ 30s |
| 10×10 | 6-8 | ≤ 5min |
| 12×12+ | ≥ 8 | ≤ 30min (尽力而为) |
