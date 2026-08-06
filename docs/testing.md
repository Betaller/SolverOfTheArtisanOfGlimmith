# 测试计划

## 1. 测试策略

### 1.1 测试金字塔

```
        ╱╲
       ╱  ╲          系统测试 (E2E)
      ╱    ╲         [pytest + 完整谜题]
     ╱━━━━━━╲
    ╱        ╲      集成测试
   ╱          ╲     [规则组合 + 模块间]
  ╱━━━━━━━━━━━━╲
 ╱              ╲  单元测试
╱                ╲ [模型 + 约束 + 形状匹配]
╱━━━━━━━━━━━━━━━━━━╲
```

| 层级 | 目标覆盖率 | 执行频率 | 运行时间 |
|------|-----------|----------|----------|
| 单元测试 | ≥ 90% | 每次提交 | < 30s |
| 集成测试 | ≥ 70% | 每日/PR | < 3min |
| 系统测试 | 关键路径 | 发版前 | < 10min |

### 1.2 测试分类

| 分类 | 标记 | 说明 |
|------|------|------|
| 快速测试 | `not slow` | 无外部依赖，纯逻辑，< 1s/例 |
| 慢速测试 | `slow` | 涉及搜索或大规模数据，> 1s/例 |
| 求解器测试 | `solver` | 需要完整搜索求解 |
| UI 测试 | `ui` | 需要 Qt 事件循环 |
| 性能测试 | `benchmark` | 基准 + 性能回归 |

---

## 2. 单元测试

### 2.1 模型层测试 (`tests/unit/`)

#### 2.1.1 `test_board.py` — 棋盘数据结构

```
test_cell_creation:
  - 创建 Cell，检查 row/col/region_id 默认值
  - 验证 dataclass 不变性

test_edge_creation:
  - 创建水平 Edge，验证坐标正确性
  - 创建垂直 Edge，验证坐标正确性
  - Edge.is_boundary 默认值测试

test_vertex_creation:
  - 创建 Vertex，验证虚拟坐标正确性

test_board_connectivity:
  - 构建 3×3 Board
  - 测试 get_neighbors() 返回四邻
  - 测试 get_neighbors() 不返回对角

test_board_edges:
  - 测试 grid 中所有内部 Edge 的生成
  - 水平边数量 = H × (W-1)
  - 垂直边数量 = (H-1) × W

test_board_vertices:
  - 测试所有四格交汇点生成
  - 顶点数量 = (H-1) × (W-1)

test_region_isolation:
  - 给特定区域赋值后，验证 get_cells_in_region()
```

#### 2.1.2 `test_shapes.py` — 形状匹配引擎

```
test_shape_normalization:
  - 形状归一化到原点 (0,0)
  - 形状 [(2,2),(2,3),(3,2)] → [(0,0),(0,1),(1,0)]

test_shape_rotation:
  - 旋转 0°: 形状不变
  - 旋转 90°: (r,c) → (c, max_r - r)
  - 旋转 180°: (r,c) → (max_r - r, max_c - c)
  - 旋转 270°: (r,c) → (max_c - c, r)
  - 旋转 360° 回到原形

test_shape_flip_horizontal:
  - 水平翻转: (r,c) → (r, max_c - c)
  - 翻转后旋转四向生成 4 个变体

test_shape_flip_vertical:
  - 垂直翻转: (r,c) → (max_r - r, c)
  - 翻转后旋转四向生成 4 个变体

test_shape_canonical_form:
  - 同一形状的所有 8 种变换生成相同规范形
  - L 形和其旋转/翻转版本规范形相同
  - L 形和 I 形规范形不同

test_shape_equality:
  - 形状 S 与自身相等
  - 形状 S 与 S 的旋转版本相等
  - L 形与镜像 L 形相等
  - L 形与 I 形不相等

test_shape_pool_matching:
  - 形状池 [L, I, O] 匹配
  - L 形匹配成功
  - 田字形匹配 O 成功
  - 不规则形不匹配任何形状

test_shape_hash:
  - 相同规范形的哈希相同
  - 不同规范形的哈希不同

test_polyomino_enumeration:
  - 1 格有 1 种
  - 2 格有 1 种（多米诺）
  - 3 格有 2 种（三格骨牌: I, L）
  - 4 格有 5 种（四格骨牌: I, O, L, T, S）
  - 5 格有 12 种（五格骨牌）

test_shape_bounding_box:
  - L 形 [(0,0),(1,0),(1,1)] → 2×2 包围盒
  - I 形 [(0,0),(0,1),(0,2)] → 1×3 包围盒
```

#### 2.1.3 `test_constraints.py` — 约束定义

```
test_constraint_creation:
  - 创建各类型约束实例
  - 约束参数类型校验

test_constraint_serialization:
  - 约束 → dict → 约束 (round-trip)

test_rule_definitions:
  - 22 条规则均有唯一定义
  - 规则参数 schema 正确

test_rule_conflict_detection:
  - 相同 + 相异 → 冲突
  - 方块 + 非方块 → 冲突
  - 精确 + 范围 → 可共存（范围包含精确值时）

test_rule_prerequisites:
  - 形状池规则必须先定义形状池
  - 拼块规则的图案必须来自形状池
```

#### 2.1.4 `test_propagator.py` / `test_backtrack.py` / `test_validator.py` — 已移除

> 这三个文件随 Python 求解器栈（2026-08-06）一并删除：求解引擎已 Rust-only，
> 相关单测覆盖转移至 `tests/integration/test_solver_end_to_end.py`（Rust-only router
> 端到端）与 `src/validation/validator.py`（`IndependentValidator`）。

---

## 3. 集成测试

### 3.1 规则组合测试 (`tests/integration/test_rule_combinations.py`)

| 用例 | 规则组合 | 网格 | 说明 |
|------|----------|------|------|
| IC-01 | 面积 + 精确 | 4×4 | 面积线索 + 全局统一面积 |
| IC-02 | 面积 + 范围 | 4×4 | 面积线索 + 面积范围 |
| IC-03 | 形状池 + 拼块 | 4×4 | 形状池约束 + 单元格形状线索 |
| IC-04 | 形状池 + 相同 | 4×4 | 形状池 + 全同形状 |
| IC-05 | 形状池 + 相异 | 4×4 | 形状池 + 完全不同 |
| IC-06 | 异生 + 双生 | 4×4 | 两种边界约束共存 |
| IC-07 | 方块 + 范围 | 4×4 | 矩形 + 面积范围 |
| IC-08 | 混合 + 差异化 | 4×4 | 相邻不同形状 + 相邻不同面积 |
| IC-09 | 玫瑰窗 + 独居 | 4×4 | 两种符号约束 |
| IC-10 | 异生 + 不等号 + 差值 | 4×4 | 三种边框约束 |
| IC-11 | 砖纹 + 环纹 | 4×4 | 两种顶点约束 |
| IC-12 | 罗盘 + 形状池 | 6×6 | 罗盘距离 + 形状约束 |
| IC-13 | 望塔 + 范围 | 4×4 | 顶点约束 + 面积范围 |
| IC-14 | 围栏 + 相同 | 4×4 | 边界图案 + 全同形状 |
| IC-15 | 精确 + 范围 | 4×4 | 精确值在范围内 |
| IC-16 | 方块 + 形状池 | 4×4 | 矩形必须匹配形状池 |

### 3.2 端到端求解测试 (`tests/integration/test_solver_end_to_end.py`)

```
test_solver_empty_puzzle:
  - 2×2 无规则 → 找到解（任一种划分均可）

test_solver_single_cell_regions:
  - 4×4 + 精确=1 → 每格独立
  - 计算区域数 = 16

test_solver_precise_tiling:
  - 4×4 + 精确=4 → 4 个 4 格区域
  - 检查区域数 = 4

test_solver_block_partition:
  - 6×6 + 方块 → 全矩形
  - 检查所有区域为矩形

test_solver_shape_pool_basic:
  - 4×4 + 形状池 [L形, I形]
  - 找到匹配的解

test_solver_compass_simple:
  - 3×3 + 罗盘中心格 (1,1,1,1)
  - 每个方向 1 格同色

test_solver_watchtower_simple:
  - 2×2 + 中心望塔=4
  - 4 格全不同区域
```

---

## 4. 系统测试

### 4.1 完整谜题测试 (`tests/system/test_full_puzzles.py`)

系统测试使用预定义的完整谜题文件（存放在 `puzzles/tutorials/` 和 `puzzles/official/`）。

测试流程：
1. 加载谜题 JSON
2. 构造 Puzzle 对象
3. 初始化求解器
4. 求解
5. 验证结果
6. 验证性能

测试数据命名约定：
```
puzzles/tutorials/
├── area_01.json           # 规则8: 面积，4×4
├── block_01.json          # 规则14: 方块
├── shape_pool_01.json     # 规则1: 形状池
└── ...

puzzles/official/
├── level_01.json          # 组合规则
├── level_02.json
└── ...
```

| ID | 描述 | 网格 | 规则 |
|----|------|------|------|
| ST-01 | 面积教学关 | 4×4 | 面积(规则8) |
| ST-02 | 精度教学关 | 4×4 | 精确(规则5) |
| ST-03 | 形状池基础 | 6×6 | 形状池(规则1) |
| ST-04 | 方块分区 | 6×6 | 方块(规则14) |
| ST-05 | 官方关卡 1 | 8×8 | 形状池+面积+范围 |
| ST-06 | 官方关卡 2 | 8×8 | 形状池+异生+双生+不等号 |
| ST-07 | 官方关卡 3 | 10×10 | 玫瑰窗+形状池+范围+砖纹 |

### 4.2 性能基准测试 (`tests/system/test_performance.py`)

```
test_benchmark_4x4_basic:
  - 4×4, 面积规则
  - 目标: ≤ 1s
  - 采样: 10 次

test_benchmark_6x6_shape_pool:
  - 6×6, 形状池 + 范围
  - 目标: ≤ 5s
  - 采样: 5 次

test_benchmark_8x8_mixed:
  - 8×8, 形状池 + 面积 + 范围 + 异生
  - 目标: ≤ 30s
  - 采样: 3 次

test_benchmark_memory_8x8:
  - 8×8 求解过程内存峰值监控
  - 目标: ≤ 512MB

test_regression_previous_solutions:
  - 所有已求解过的谜题再次求解
  - 确保性能不比历史记录差 20% 以上
```

### 4.3 UI 测试 (`tests/system/test_ui_basic.py`)

```
test_ui_window_creation:
  - 创建 MainWindow
  - 验证窗口标题为 "格里米斯的工匠 - 求解器"
  - 验证默认大小 ≥ 1280×720

test_ui_grid_creation:
  - 创建 6×6 空网格
  - 验证网格显示正确
  - 验证单元格数量

test_ui_tool_selection:
  - 切换各工具模式
  - 验证工具状态

test_ui_rule_panel:
  - 启用/禁用规则
  - 验证规则状态同步

test_ui_solver_integration:
  - 加载简单谜题
  - 点击求解按钮
  - 验证求解完成后网格更新
```

---

## 5. 测试基础设施

### 5.1 Fixtures (`tests/conftest.py`)

```python
@pytest.fixture
def empty_puzzle_4x4() -> Puzzle:
    """4×4 空谜题，无规则无线索。"""
    ...

@pytest.fixture
def puzzle_with_area_clues() -> Puzzle:
    """4×4 谜题，带面积数字线索。"""
    ...

@pytest.fixture
def puzzle_with_shape_pool() -> Puzzle:
    """6×6 谜题，带形状池。"""
    ...

@pytest.fixture
def sample_shapes() -> dict[str, Shape]:
    """预定义的常见形状集 {L, I, O, T, S}。"""
    ...

@pytest.fixture
def qt_app(qapp):
    """PySide6 QApplication fixture。"""
    return qapp
```

### 5.2 辅助工具

```python
# tests/helpers.py

def puzzle_from_grid(grid: list[list[int]]) -> Puzzle:
    """从二维数组快速创建 Puzzle（数字为区域 ID）。"""

def random_puzzle(rows: int, cols: int, rules: list[Rule]) -> Puzzle:
    """随机生成指定尺寸和规则的谜题。"""

def assert_regions_match(puzzle: Puzzle, solution: Solution,
                          expected_regions: list[list[int]]):
    """断言区域分配与期望一致。"""

def assert_solution_valid(puzzle: Puzzle, solution: Solution):
    """断言解通过所有规则验证。"""
```

### 5.3 Mock 对象

```python
# tests/mocks.py

class MockSolver:
    """模拟求解器，返回预设解。"""

class MockPuzzleService:
    """模拟谜题服务。"""
```

---

## 6. 测试数据

### 6.1 形状测试数据

```python
# 五格骨牌 (Pentominoes)
SHAPE_I = Shape(cells=[(0,0),(0,1),(0,2),(0,3),(0,4)])
SHAPE_L = Shape(cells=[(0,0),(1,0),(2,0),(3,0),(3,1)])
SHAPE_Y = Shape(cells=[(0,0),(1,0),(2,0),(2,1),(3,0)])
SHAPE_N = Shape(cells=[(0,0),(0,1),(1,1),(2,1),(2,2)])
SHAPE_V = Shape(cells=[(0,0),(1,0),(2,0),(2,1),(2,2)])
SHAPE_T = Shape(cells=[(0,0),(0,1),(0,2),(1,1),(2,1)])
SHAPE_U = Shape(cells=[(0,0),(0,2),(1,0),(1,1),(1,2)])
SHAPE_W = Shape(cells=[(0,0),(1,0),(1,1),(2,1),(2,2)])
SHAPE_Z = Shape(cells=[(0,0),(0,1),(1,1),(2,1),(2,2)])
SHAPE_F = Shape(cells=[(0,1),(1,0),(1,1),(2,1),(2,2)])
SHAPE_P = Shape(cells=[(0,0),(0,1),(1,0),(1,1),(2,0)])
SHAPE_X = Shape(cells=[(0,1),(1,0),(1,1),(1,2),(2,1)])
```

### 6.2 谜题测试数据

```python
# puzzles/tutorials/area_01.json
{
  "version": "1.0",
  "grid": {"height": 4, "width": 4},
  "cells": [
    {"row": 0, "col": 0, "number": 4},   # 左上区域面积=4
    {"row": 2, "col": 2, "number": 4}    # 右下区域面积=4
  ],
  "rules": [{"type": "area", "params": {}}]
}
# 期望解: 两个 2×2 区域（左上 4 格，右下 4 格）
```

---

## 7. 测试执行

### 7.1 运行全部测试

```powershell
# 运行所有测试
pytest

# 运行并生成覆盖率报告
coverage run -m pytest
coverage report
coverage html          # 生成 HTML 报告

# 运行特定分类
pytest -m "not slow"   # 快速测试
pytest -m "solver"     # 求解器测试
pytest -m "ui"         # UI 测试
pytest tests/unit/     # 单元测试
pytest tests/integration/  # 集成测试
pytest tests/system/   # 系统测试

# 运行特定文件
pytest tests/unit/test_shapes.py -v

# 运行特定测试函数
pytest tests/unit/test_shapes.py::test_shape_rotation -v

# 失败时立即停止
pytest -x

# 慢速测试超时控制
pytest --timeout=60 -m "slow"
```

### 7.2 持续集成

```yaml
# .github/workflows/test.yml
name: Test
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      - run: pip install -r requirements-dev.txt
      - run: ruff check src/
      - run: mypy src/
      - run: pytest -x -q --timeout=60
      - run: coverage run -m pytest -x -q --timeout=60
      - run: coverage report --fail-under=80
```

---

## 8. 缺陷分类与报告

### 8.1 缺陷严重等级

| 等级 | 定义 | 响应时间 |
|------|------|----------|
| S0 阻塞 | 程序崩溃、数据丢失、求解结果错误 | 立即修复 |
| S1 严重 | 主要功能不可用，无替代方案 | 24h 内 |
| S2 一般 | 功能异常但有替代路径 | 下一个迭代 |
| S3 轻微 | 界面显示问题、非关键功能 | 积压处理 |

### 8.2 Bug Report 模板

```markdown
## 缺陷报告

**环境:** Windows 10 / Python 3.12 / PySide6 6.6

**严重等级:** S1

**描述:**
精确规则下，4×4 网格设定精确=4 时求解器返回无解，但预期存在划分。

**复现步骤:**
1. 创建 4×4 空网格
2. 启用「精确」规则，参数设为 4
3. 点击求解
4. 提示"无解"

**预期结果:**
求解器应返回 4 个 4 格区域的划分方案。

**实际结果:**
提示无解。

**日志:**
[粘贴相关日志]

**截图/附件:**
[粘贴截图]
```
