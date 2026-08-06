# 04 · AoG 求解器：形状放置 + 强力剪枝的 DFS

> 阅读对象：想理解主力求解器的人。
> 前置：01（路由）、02（位域网格）、03（规则→Config）。
> 代码：`rsolver/src/solver/aog/*`，约 3600 行，是 C++ 参考求解器
> `third_party/AoG_Solver` 的 **1:1 移植**。

---

## 1. 总体思路（先看结论）

传统求解器是“**逐格分配区域**”；AoG 反过来，是“**逐个放置区域**”：

```
把棋盘想象成一个拼图板：
  · 有一个形状目录（形状库），以及一个"自由形状生成器"
  · 每次挑一个"空区锚点"（最受约束的空格）
  · 尝试放一个形状（从目录拿 或 现场生长），填满若干格
  · 每放完一个形状，立刻对空区做可行性分析，不行就回溯
  · 直到所有格子填满 = 找到解
```

为什么快？因为：

1. **起手点很聪明**（`find_special_start_area`）：从被边界/线索卡死的格子开始放，
   而不是随便挑个格子。
2. **每放完一片就做空区检查**（`empty_area_check`）：如果剩余空格形成孤岛、面积对
   不上、符号配不平，立刻整枝回溯。
3. **大量增量检查**：放每一个格时都检查边约束、望塔、T 字/十字交点等，把错误尽早掐死。

---

## 2. 数据结构速览

### 2.1 `AoGCore`（全局状态，`aog/core.rs:12-37`）

```rust
pub struct AoGCore {
    n_row, n_col: usize,              // 逻辑网格尺寸
    config: Config,                   // 规则开关（见 03）
    puzzle: Vec<Vec<u32>>,            // 位域像素网格（只读线索，2H+5 × 2W+5）
    puzzle_compass_*: Vec<Vec<i32>>,  // 四个罗盘数组
    slash_check_enable: bool,
    slash_nodes: Vec<Vec<Node>>,      // 玫瑰窗：每种符号一组的格子
    shape_size_nodes: Vec<Node>,      // 带面积数字的格子（按数字升序）
    all_shapes_same_check_shape_index: i32,      // same 规则当前形状
    all_shapes_different_check_shape_index_pool: HashSet<u32>, // different 规则已用形状
    shapes: Vec<Shape>,               // 形状目录（自由多连块，去重）
    shape_size_by_index: Vec<usize>,  // 形状索引 → 面积
    shape_digest_index: HashMap<u32, Vec<usize>>, // 摘要 → 候选索引列表
    node_to_shape_index: HashMap<(i32,i32), Vec<usize>>, // 相对坐标 → 形状索引
    next_shape_index: u32,            // 下一个形状类索引
    dfs_ctx: DfsContext,              // 空区分析的临时状态
    deadline: Instant,                // 超时
    rose_type_count: usize,           // 玫瑰窗符号类型数
}
```

### 2.2 形状目录（`Shape` + digest）

形状存放在 `shapes: Vec<Shape>`。`Shape`（`aog/types.rs:74-145`）有：

- `shape_index`：类的唯一编号（旋转/镜像去重后共享同一编号）；
- `nodes: Vec<Node>`：相对坐标列表（以第一个点为原点）；
- `digest: u32`：快速指纹；
- `preview: Vec<u32>`：逐行二进制位图。

**指纹 `digest`**（`aog/core.rs:97-129`，`compute_digest`）把形状压缩成一个 u32：

```
把形状放进最小包围正方形，逐行读成二进制行位图 preview[i]
规范化：左对齐（<<= most_left_j）、上对齐（移位到 0 行）
然后滚动哈希：d = d * 131 + preview[i]
```

`shapes_search(shape)` 先按 digest 查 `shape_digest_index`，再逐候选 `eq_shape`
比对 preview，命中返回 shape_index。

`shapes_insert`（`aog/core.rs:168-192`）把一个 0/1 网格的 **8 种朝向**（4 旋转 × 2 镜像）
都尝试插入目录，去重后只保留一个类，`next_shape_index` 递增。

> 所以同一形状的不同朝向在目录里是**同一个索引**——这正是“旋转和镜像视为同一形状”
> 的落地方式。

### 2.3 `PlaceLevel`（每层搜索的工作区，`aog/types.rs:224-289`）

```
每个 DFS 深度 index 一个独立的 PlaceLevel：
  current_shape: [Node; MAX_SHAPE_SIZE]      当前正在生长的形状
  current_shape_cnt                          已放格子数
  expand_candidates: [Node; MAX_EXPAND_CANDIDATES]  可扩展候选
  expand_candidates_distance                   候选距离（BFS 层数）
  rectangle_*: [i32; MAX_SHAPE_SIZE]         块规则：包围盒四边
  palisade_visited_*                          已访问的围栏格
  compass_visited_* + 四个方向计数数组         已访问的罗盘格
  stack_*: 多个定长栈                         迭代 DFS 的显式栈
  symbol_loc: Option<Node>                   独居规则：本区符号位置
  mark_slash / slash_node_indexs / slash_dist_buf   玫瑰窗斜线距离剪枝
```

用 `RefCell`（`Pools`）包起来，保证不相交的层级可独立借用。

### 2.4 `DfsContext`（空区分析状态，`aog/types.rs:186-220`）

```rust
visited: Vec<Vec<i32>>,           // 访问标记（visited_index 代代递增，省 memset）
empty_count, empty_block_line_count,
empty_block_line_node_pairs: BTreeSet<(Node,Node)>,   // 连通分量里穿过的预画边界对
symbol_count, slash_count: [i32;10],
compass_nodes, compass_node_states,     // 罗盘格 + 计数
area_shape_sizes,                       // 连通分量里的面积数字
block_adj: HashMap<u64,Vec<Node>>,      // 二分图：边界两侧配对
place_visited: HashMap<u64,i32>,
```

---

## 3. 主流程：`solve_aog`

`aog/mod.rs:21-42`：

```
solve_aog(puzzle, deadline)
│
├─ AoGCore::build(puzzle, deadline)      # 编码位域网格、注册形状目录
├─ core.make_solve_puzzle()              # sp = 全 LINE_BLOCK，内部 AREA_NORMAL/BLOCK
├─ Pools::new(MAX_DFS_DEPTH)             # 预分配 100 层工作区
├─ search::dfs(1, &mut core, &mut sp, &pools)    # 从深度 1 开始搜
│     │
│     └─ 返回 -1？ → 无解；否则 → extract_regions
├─ extract_regions(&core, &sp, puzzle)   # 位域 → Vec<RegionInfo>
└─ validate::validate(puzzle, &regions)  # 出口复核，失败返回 None
```

---

## 4. 核心递归：`dfs(index, core, sp, pools)`

`search.rs:839-1313`。每一层**放一个区域**。伪代码：

```
dfs(index):
  ① 若超时 / index 超深 → 返回 -1
  ② (ret, x, y) = find_special_start_area(core, sp)   # 选起手点
     若 ret==DEFAULT 且 x==-1 → 所有格已填 → 返回 0（成功）
  ③ 计算本层的尺寸可行域 mk_size[size]（结合空区上下界 + 规则上下界）
  ④ 对 size 在可行域内：
       a. 若起手点带面积数字 → 只允许该 size
       b. 若起手点邻接不等号/差值边且邻居已放 → 用邻居尺寸过滤 mk_size
  ⑤ 遍历形状目录 shapes[cur]：
       Type1 检查：平移到 (x,y) 后所有格是否在界内、未占、不冲突
       Type2 检查：逐格写 sp，检查边/边约束/相邻异形/异面积
       Type3 检查：围栏、望塔、T字/十字交点
       Type4 检查：empty_area_check 空区可行性
       递归 dfs(index+1)，成功直接上抛
  ⑥ 若起手点是"自由锚点"（DEFAULT）：
       对每个可行 size 调 place_non_predifined_shape(index,x,y,size,...)
       现场生长形状（自由多连块）
  ⑦ 都不行 → 返回 -1
```

### 4.1 空区尺寸可行域 `mk_size`

每一层会重新算 `(rlb, rub) = empty_area_size_range(x,y,core,sp)`：

- 用 `dfs_empty_area` 泛洪当前空区，得到 `empty_count`（连通格数）和
  `empty_block_line_count`（空区内被预画边界隔开的“墙”最少要切几刀）；
- `max_area_size = empty_count - empty_block_line_count` 是**该空区能放的最大区域面积**；
- 再与规则上下界（`shape_size_lower_bound/upper_bound`）取交集，标记到 `mk_size[]`。

> 如果 `_empty_area_shape_count == 1`，说明该空区只有一个可能的大小（比如恰好等于
> 一个面积数字、或整区就是一个区域），就把上下界都锁定为 `max_area_size`。
> 这就是“孤岛必是整块”的剪枝。

### 4.2 特殊起手点 `find_special_start_area`

`empty.rs:771-829`，按优先级从强到弱找锚点，返回 `(类型, x, y)`：

```
优先级  类型                         找到的条件（在空格中找）
──────────────────────────────────────────────────────────
高     SIZE_1_REGION    四周全被占/隔断的孤格（只能自己成一区）
      SIZE_MATCH_REGION 空区大小正好等于规则尺寸的单区
      LINE_SAME         邻接一条"双生="线且邻居已放的空格
      LINE_SMALLER_OR_LARGER  邻接不等号且邻居已放的空格
      AREA_INDEX        带拼块图案索引的空格
      AREA_SIZE         带面积数字的空格（shape_size_nodes 按升序取）
      COMPASS           带罗盘的空格
      LINE_CONSTRAINT   邻接任何约束边的空格
      CORNER            至少 3 条"墙"包围的角落格
低     DEFAULT          第一个任意空格
```

对应 `SPECIAL_START_*` 常量（`aog/types.rs:37-47`）。不同的起手类型在 `dfs` 里
触发不同的处理：如 `AREA_SIZE` 把尺寸锁定为该数字；`LINE_SMALLER_OR_LARGER` /
`LINE_SIZE_DIFF` 用邻居区域面积过滤 `mk_size`。

> 起手顺序直接决定搜索树形状。`find_empty_line_constraint_area` 的实现有一条
> 长注释（`empty.rs:729-756`）解释为什么它**不像**其它 finder 那样要求邻居已放——
> 那是为了精确复刻 C++ 的搜索顺序（曾经少这一个分支导致某些题永远搜不到解）。

---

## 5. 自由形状放置：`place_non_predifined_shape`

当锚点是 DEFAULT（无特殊约束），aog 不再从目录挑形状，而是**现场生长一个连通多连块**。
`search.rs:51-835`，一个**迭代式 DFS**（用显式栈代替递归）。

### 5.1 生长算法

```
place_non_predifined_shape(index, x, y, size, up_left_seq, known_shape_index, ...):

  工作区初始化：current_shape=[(0,0)]，expand_candidates=[(0,0)]，stack 压入初始帧

  while stack 非空:
    弹出一帧 (current_size, 候选下界, ...)

    ▸ 回滚：把 current_shape 里超过 current_size 的格清掉（写回 AREA_NORMAL，
      维护 rose/围栏/罗盘计数）

    ▸ 若 current_size == size（形状长满了）:
        - 校验非矩形/独居/玫瑰窗等"整形状条件"
        - 把 current_shape 组装成最小正方形位图 → shapes_search 找目录索引
          （找不到就现场插入目录）
        - 写 sp：每格 |= shape_index << 16
        - 逐格检查 边约束/相邻异形/相邻异面积/拼块索引/T字/十字/望塔
        - 再检查已访问的 围栏格/罗盘格
        - 全过 → empty_area_check → 递归 dfs(index+1)
        - 失败 → 清掉 shape_index 位，continue

    ▸ 否则（形状还没长满）:
       遍历 expand_candidates[from..]：
          - 有序下界剪枝（expand_distance_lb / x_lb / y_lb，保证形状按规范序生长，
            避免重复枚举同一形状的不同"长出顺序"）
          - 若候选是面积数字格且数字≠size → 跳过
          - 写 sp[候选]=index；check_edge（不能跨预画边/和本区其它格隔着边界）
          - 玫瑰窗：同一符号类型不能出现两次
          - 维护 rectangle_* / 围栏 / 罗盘 增量数据
          - 增量检查：矩形越界、围栏type1、罗盘超限、斜线距离剪枝
          - 独居：如果已有符号位置且候选也是符号 → 跳过
          - 把候选的 4 邻域没被占的新格加入 expand_candidates
          - 压两帧：a) 跳过此候选继续枚举；b) 接受此候选向下生长
          - break（先把当前候选的长分支跑完）

  全部耗尽 → 返回 -1
```

### 5.2 为什么用显式栈 + 距离下界？

两个目的：

1. **避免递归爆栈**：`MAX_STACK_SIZE = 258` 深度的迭代栈代替递归。
2. **规范序去重**：`expand_candidates_distance` 记录每格的 BFS 距离，
   `expand_distance_lb / expand_x_lb / expand_y_lb` 记录当前帧的最小可接受坐标。
   只有当候选的 `(距离, x, y)` 按字典序 ≥ 下界时才接受——这保证同一个形状
   不会被“不同的长出顺序”重复枚举（对称性破缺）。

---

## 6. 空区分析：`empty_area_check` 与 `dfs_empty_area`

这是 aog 剪枝的**灵魂**。每放完一个区域后对每个空区做全套可行性检查。

### 6.1 `dfs_empty`（`empty.rs:10-93`）—— 泛洪

从起点出发，**沿着“没有分界线的边”** 4 邻域扩散，统计：

- `empty_count`：连通空格数；
- 途中遇到的“预画边界”配对记录进 `empty_block_line_node_pairs`；
- `symbol_count` / `slash_count[]` / `area_shape_sizes` / 罗盘格。

### 6.2 `dfs_empty_area`（`empty.rs:186-254`）—— 关键差值

```rust
empty_block_line_count += 最小切割数
```

对每个“穿空区的预画边界”，把它的两端格加入二分图 `block_adj`，然后
`try_place_id` 对每个连通块尝试 0/1 染色（二分图二部划分），取**两种染色里
节点更少的一方**，累加进 `empty_block_line_count`。

```
含义：
  一个空区里如果有 k 条预画边界穿过，那么这些边界把空区分成若干小块；
  空区最大能容纳的区域面积 ≤ empty_count - (边界数)。
```

### 6.3 `empty_area_check`（`empty.rs:332-472`）—— 全部剪枝

对每个尚未填的空区依次做：

| # | 检查 | 拒绝条件 |
|---|---|---|
| 0 | ring T 字剪枝 `ring_t_junction_check` | 环纹规则下，顶点四格恰好 1 空、3 边界 → 死局 |
| 1 | 尺寸下界 | `empty_count - block_line_count < 尺寸下界` |
| 2 | 面积数字求和 | 空区内不重复的面积数字之和 > 空区大小 |
| 3 | 独居 | 空区无符号；或空区恰 1 符号但还隔着预画边 |
| 4 | 玫瑰窗 | 每种符号计数不相等、或为 0、或 1 个但隔边 |
| 5 | 精确尺寸整除 | 上下界相等时：`empty_count % lb != 0` |
| 6 | same 规则整除 | 空区大小 % 已定形状面积 != 0 |
| 7 | 罗盘 | `dfs_empty_compass_check` 逐方向比对是否可能满足 |
| 8 | 围栏（空格上） | 空格上围栏类型与相邻空格方向矛盾 |

---

## 7. 增量检查函数大全

| 函数 | 位置 | 检查什么 |
|---|---|---|
| `check_edge` | `aog/core.rs:316` | 同区域格不能跨 `LINE_BLOCK` |
| `check_edge_shape` | `aog/core.rs:264` | 边约束：双生/异生/不等号/差值 |
| `check_nearby_shape` | `aog/core.rs:225` | mixed：相邻区域形状不同 |
| `check_nearby_size` | `aog/core.rs:243` | differentiation：相邻区域面积不同 |
| `check_palisade_type1` | `aog/core.rs:362` | 围栏（部分填时） |
| `check_palisade_type2` | `aog/core.rs:332` | 围栏（整形状后） |
| `check_tatami` | `aog/core.rs:392` | brick：无 4-way 交点 |
| `check_loopy` | `aog/core.rs:409` | ring：无 3-way 交点 |
| `check_radar` | `aog/core.rs:438` | watchtower：顶点区域数 |
| `ring_t_junction_check` | `empty.rs:274` | ring：T 字剪枝（空区视角） |

---

## 8. 三个“额外移植”的剪枝（CLAUDE.md 提到的）

### 8.1 ring T 字剪枝（禁 T）

`ring_t_junction_check`（`empty.rs:274-330`）：环纹规则下，考虑一个顶点周围的 4 格，
如果其中**恰好 1 格还是空格**、其余 3 格已定且彼此恰好形成 3 条边界——那么这个空格
将来必然成为新区域，它在该顶点必然补成 3 条边界（T 字），环纹禁止 → 死局。

```
    ┌───┬───┐
    │ A │ B │        空格 O 将被新区域占据 → 顶点处三条边界
    ├───┼───┤        → 环纹禁止 → 剪枝
    │ C │ O │   (A,B,C 属于不同区域，空格 O)
    └───┴───┘
```

### 8.2 slash-distance 剪枝（玫瑰窗）

`search.rs:632-693`：玫瑰窗规则下，形状还需“够得着”每种未放的符号。维护
`slash_dist_buf`（每个已放格到每种符号最近格的距离），枚举每种未放符号取一个代表格，
计算“最小包围直径” `distance_predict`；如果

```
distance_predict > 剩余格数
```

则该形状无论如何都长不到能同时覆盖所有符号，剪枝。（对应 C++ `dfs.cpp` 1260-1306。）

### 8.3 enlarged shape/stack 数组

`MAX_SHAPE_SIZE=256`、`MAX_STACK_SIZE=258`、`MAX_EXPAND_CANDIDATES=(256+2)*3`，
比 C++ 原版更大，避免大区域时静默溢出（`aog/types.rs:49-54` 注释）。

---

## 9. 流程总图

```
                    solve_aog
                        │
                        ▼
                AoGCore::build ────► 位域网格 + 形状目录
                        │
                        ▼
                dfs(index=1)
                        │
        ┌───────────────┴────────────────┐
        ▼                                ▼
  find_special_start_area          （空盘特殊情况）
        │
        ▼
  确定尺寸可行域 mk_size[]
        │
        ▼
  ┌────────────────────────────────────────────────┐
  │ 遍历形状目录 shapes[cur]                        │
  │   Type1 平移检查 → Type2 写格+边检查             │
  │   → Type3 围栏/望塔/交点 → Type4 空区检查        │
  │   → dfs(index+1)                                │
  └────────────────────────────────────────────────┘
        │ （DEFAULT 锚点时）
        ▼
  place_non_predifined_shape(index,x,y,size)
        │   迭代生长形状（规范序去重）
        │   每步增量检查 + 斜线距离剪枝
        │   长满 → 写形状索引 → empty_area_check
        │   → dfs(index+1)
        ▼
   成功返回 0 或 -1
        │
        ▼
  extract_regions → validate → RegionInfo
```

---

## 10. 本节代码索引

| 主题 | 位置 |
|---|---|
| `solve_aog` 入口 | `aog/mod.rs:21` |
| `AoGCore` 定义 | `aog/core.rs:12` |
| `compute_digest` | `aog/core.rs:97` |
| `shapes_search` / `shapes_insert` | `aog/core.rs:131, 168` |
| `PlaceLevel` / `Pools` 定义 | `aog/types.rs:224, 291` |
| `dfs` 主递归 | `search.rs:839` |
| `place_non_predifined_shape` | `search.rs:51` |
| `find_special_start_area` | `empty.rs:771` |
| `dfs_empty_area`（二分图切割数） | `empty.rs:186` |
| `empty_area_check` 剪枝清单 | `empty.rs:332` |
| `ring_t_junction_check` | `empty.rs:274` |
| 各 check_* 函数 | `aog/core.rs:225-484` |

---

下一节：[05-pieces求解器](05-pieces求解器.md)
