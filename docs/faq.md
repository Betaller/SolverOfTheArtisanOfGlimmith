# 常见问题（FAQ / 经验教训）

本文档记录开发与转换过程中踩过的坑和验证过的结论，避免重复犯错。

## 1. 谜题 JSON 边界（outer_boundaries）方向约定

### 1.1 症状

官方 Zone1-3 谜题在 UI 中边框显示错乱：部分横边框显示为竖边框（或顶部出现竖线）。

### 1.2 根因

`grid_widget.py` 判定横竖的依据是端点坐标：

- `r1 == r2` → 水平线段（横边框）
- `c1 == c2` → 垂直线段（竖边框）

转换脚本（`convert_archive.py` / `puz2json.py`）曾按 `(-1,c)-(0,c)` 生成上/下边框、
按 `(r,-1)-(r,0)` 生成左/右边框，导致：

- 上/下边框 `(-1,c)-(0,c)`：`r1 ≠ r2` → 被画成竖线（且 y 为负，跑到棋盘外）
- 左/右边框 `(r,-1)-(r,0)`：`r1 == r2` → 被画成横线

### 1.3 正确约定（与 `_outer_key` / `_draw_outer_edge` 一致）

| 位置 | 规范形式 |
|------|----------|
| 上边框 | `(0, c) - (0, c+1)`，`c ∈ [0, W)` |
| 下边框 | `(H, c) - (H, c+1)`，`c ∈ [0, W)` |
| 左边框 | `(r, 0) - (r+1, 0)`，`r ∈ [0, H)` |
| 右边框 | `(r, W) - (r+1, W)`，`r ∈ [0, H)` |

### 1.4 经验教训

- **生成数据的约定必须与消费它的渲染代码一致**。写转换器前先读 UI 的绘制/命中代码。
- 加载侧做防御：`puzzle_codec.dict_to_puzzle` 已通过 `_canonical_outer` 把任意相邻顶点的
  边界段规范化为 `(r,c,r,c+1)`（横）或 `(r,c,r+1,c)`（竖），防止端点反序破坏渲染。
- 修改转换脚本后务必重新生成数据并校验：重新转换后共 35470 条边界段、0 条非法。

## 2. 官方谜题库转换（puzzles.json → 项目格式）

源：`third_party/archiveofglimmith.github.io/puzzles.json`（1231 题，Zone1-3）。
转换脚本：`scripts/convert_archive.py`。

### 2.1 网格文本解析要点

- 网格共 `2*H+1` 行：偶数行是墙行（固定 stride=3），奇数行是格子行。
- 格子行必须用 **greedy 解析**（同 `index.html` 的 `parseCellRow`）：
  - 格子内容以 `U` 开头 → 罗盘串，读到 `R\d*` 为止（如 `U2D3L0R`、`U1DLR`）。
  - 以 `S` 开头 → 形状标记，可能有多位 id（如 `S10`），要按 shapes 表回退匹配。
- 墙行字符：`##`/`#` 预画边界、`--`/`|` 普通、`==`/`=` 双生(homogeneous)、
  `!!`/`!` 异生(heterogeneous)、`^^`/`<`/`^` 不等号(inequality)、`vv`/`>`/`v` 反向不等号、
  `-N`/数字 差值(difference，值为显示值本身)。
- 顶点数字（vertex-radar）出现在墙行 corner 位置 `3*c`。

### 2.2 规则映射表

| archive 字段 | 项目 rule |
|--------------|-----------|
| `shape_bank`（有 shapes 时） | `shape_pool`（仅 bank 内的形状） |
| 格子里的 `S#`（非 1SPR） | `cell.shape_pattern` + `puzzle_piece` |
| `one_symbol_per_region` | `solitary`（所有 clue 格同时记 `symbol` 原始串） |
| `area_equals` | `precise` |
| `area_at_least` / `area_at_most` | `range` |
| `all_shapes_same` / `different` | `same` / `different` |
| `adjacent_shapes_different` | `mixed` |
| `adjacent_sizes_different` | `differentiation` |
| `only_rectangles` / `no_rectangles` | `block` / `non_block` |
| `no_4_way_intersections` / `no_3_way_intersections` | `brick` / `ring` |

### 2.4 官方解（answer 文件）

archive 每个官方题都带 `solution` 字段（紧凑边界网格，stride=3，`#`/`##` 为墙）。
`scripts/convert_answers.py` 把它解码成区域划分，按谜题布局镜像写入
`puzzles/official/{zone}-answer/{type}/{id}.json`：

```json
{
  "version": "1.0",
  "grid": {"height": 6, "width": 5},
  "regions": [[[1, 0], [2, 0], ...], ...],
  "_meta": {"archive_id": "0008", "archive_type": "1-single-shape", "archive_difficulty": 1}
}
```

- 解码与校验逻辑在 `convert_archive.archive_solution_regions`（保证覆盖所有可填格、
  每区域四连通、不含障碍格），转换脚本与 `convert_archive.py` 共享同一实现。
- 无官方解的题（0067 / 1130）会被跳过。
- 官方解是**唯一解**：每个官方题有且只有一个合法解，answer 文件即该题的唯一解。

### 2.3 经验教训

- **shapes 不一定是全局形状池**：`8-poly`、`10-same-shape-no-touch` 等类型中 `shapes`
  只是 `S#` 格子的图标，区域形状只由 `S#` 格约束（→ `puzzle_piece`），不能用 `shape_pool`。
  只有带 `shape_bank` 时才表示“区域必须是池中形状”。
- **1SPR 的 clue 都是符号**：`one_symbol_per_region` 时，数字/`S`/`F`/罗盘格都算“符号”，
  需同时写 `cell.symbol`（原始串），否则 `check_rule_solitary` 每区域 0 符号而误判失败。
- **围栏 F 值映射与 .puz 相反**：archive 中 F2=2 个相对边界、F7=2 个相邻边界；
  puz 格式正好相反。映射（经官方 solution 验证）：
  F0=0、F1=1、F2=2相对、F3=3、F4=4、F7=2相邻。
- **罗盘语义是半平面计数**：游戏规则（`glimmith-solver` 的 `compassCounts` 证实）统计
  该区域内**所有**严格位于该方向的格子（排除自身），不是直线连续计数。项目
  `check_rule_compass` 用的是直线计数且有测试锁定，**不要改**——转换数据保持官方原值，
  罗盘题在本项目求解/校验时会不一致，属已知限制。
- **围栏格与障碍格相邻时**：项目 `check_rule_fence` 不把 blocked 邻居算作边界，游戏会算。
  不规则棋盘上的围栏格可能校验失败，属项目实现限制，不是转换错误。
- **验证方法**：把 archive 的 `solution`（紧凑格式，stride=3，`#`/`##` 为墙）反推成区域，
  填充 `Board` 后跑 `IndependentValidator`（`src/validation/validator.py`）对照。转换质量以
  “官方解能通过校验”为准。

## 3. Git 子模块（third_party）

- 第三方依赖统一放 `third_party/`，用 `git submodule add <url> third_party/<name>`。
- **Windows 换行陷阱**：子模块仓库内若有超大单行文件（如 1.2MB 的 `puzzles.json`），
  Git 的 LF→CRLF 处理会把整个文件标记为已修改。此时 `git -C <submodule> checkout -- <file>`
  还原即可；不要把这个换行变更提交进子模块。
- 添加子模块后 `git status` 会出现 `A third_party/xxx`（gitlink）+ `.gitmodules` 修改，
  一并提交。

## 4. 验证命令速查

```powershell
python scripts/convert_archive.py            # 转换官方题（会清空并重写 Zone1-3）
python scripts/convert_archive.py --dry-run  # 只校验不落盘
python -m pytest tests/ -x --tb=short       # 全量测试
python scripts/verify_puzzles.py             # 求解验证（大目录会很久）
```
