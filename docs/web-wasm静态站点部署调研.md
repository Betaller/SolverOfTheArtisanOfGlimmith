# WebAssembly 静态站点部署可行性调研

> 目标：把《格里米斯的工匠》求解器部署为**纯静态网站**，由 **GitHub Pages** 托管，
> 求解引擎通过 **WebAssembly (WASM)** 在浏览器内运行。本文只做调研与方案设计，
> 不含实现代码。调研日期：2026-08-14。

---

## 0. 结论摘要（TL;DR）

**可行，且 Rust 求解器几乎零改动即可编译到 WASM。** 工作量主体不在求解器，而在**新写一份 Web 前端**。

| 模块 | 现状 | 迁移成本 | 结论 |
|---|---|---|---|
| Rust 求解器 (`rsolver/`) | 纯计算引擎，仅 `serde`/`serde_json` 两个依赖，无线程/系统库 | 极低 | **直接编译 wasm32，只差一个时钟适配** |
| Python 求解层 (`src/solver/`) | 仅剩 router + Rust 子进程包装 | 低 | 丢弃，改为 JS 直接调 WASM |
| Python 校验层 (`src/validation/`) | `IndependentValidator` | 低 | 用 Rust 侧 `validate.rs` 替代，无需移植 |
| Python 模型/编解码 (`src/models/`、`src/io/`) | JSON 协议 + 模型 | 低 | JS 侧按既有 JSON 协议产出，无需移植 |
| **Python UI (`src/ui/` + PySide6/Qt)** | 完整编辑器 | **高** | **不移植，单独写一份 Web UI**（用户已确认） |

建议技术栈：**React 或 Vue 3 + TypeScript + Vite** + `wasm-bindgen`/`wasm-pack` + Web Worker + GitHub Actions 部署 GitHub Pages。

---

## 1. 现状盘点

### 1.1 Rust 求解器：天然可移植

`rsolver/Cargo.toml` 只有两个依赖：

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

无 `rayon`/`thread`/`std::net`/`std::fs`（除入口的 file 分支）/系统调用。整个 `rsolver/src/`
是纯 CPU 计算：`aog`（DFS 直接移植 C++ 参考）、`pieces`（DLX 精确覆盖）、`backtrack`（逐区域 DFS）、
`rose`（玫瑰窗专用）、`validate`（完整独立验证器）、`dlx`/`polyomino`/`shapes`/`types`/`grid`/`io`。

结论：这是一个**无宿主依赖的确定性计算引擎**，是 WASM 移植的理想对象。

### 1.2 Python 栈：四层，三层可弃

按 `src/` 目录逐层判定：

| 层 | 文件 | 浏览器命运 |
|---|---|---|
| UI | `src/ui/**`（Qt/PySide6：grid_widget、main_window、shape_editor、constraint_panel …） | **弃用，新写 Web UI** |
| 路由 | `src/solver/base.py`、`rust_solver.py` | **弃用**（子进程/`select`/`subprocess` 是宿主专有） |
| 校验 | `src/validation/validator.py`、`src/solver/constraints.py` | **弃用**，由 Rust `solver/validate.rs` 替代 |
| 模型/编解码 | `src/models/**`、`src/io/puzzle_codec.py` | **弃用**，JS 侧按 JSON 协议直接产出 |

其中 `rust_solver.py` 里的 `_fitting_rectangles`（`block` 规则合成矩形池）是一个**需要留意的前置逻辑**：
它把「矩形区域」转成 shape_pool 喂给 DLX。Web 版要么在 JS 侧复刻这段（约 40 行），要么在 Rust 侧补一个
`block` 前置（更干净，见 §8 路线图）。

### 1.3 可直接复用的参考资产（third_party）

- **`glimmith-solver`**（JS 子模块）：浏览器内原型，**已实现全 22 规则的编辑 UI + 精确覆盖求解 + 导入导出**，
  无框架、无运行时依赖（纯 HTML/CSS/JS）。可作为 UI/规则编辑器的**参考实现**，但求解能力弱（仅精确覆盖原型，
  解不了本项目 2000+ 官方题的绝大多数）。
- **`archiveofglimmith.github.io`**（GitHub Pages 子模块）：官方爱好者站点，`index.html` + `puzzles.json`(1.2MB)。
  这是「该游戏静态托管在 GitHub Pages」的**既有先例**，可作题库浏览层参考。

---

## 2. WASM 可移植性逐项分析

把 `rsolver/src/` 里所有「宿主环境 API」逐一列出，判定其在 `wasm32-unknown-unknown` 上的行为：

| 宿主 API | 用途 / 位置 | wasm32-unknown-unknown 行为 | 对策 |
|---|---|---|---|
| `std::env::var(...)` | `AOG_DEBUG`/`AOG_ONLY`/`BF_PROPAGATE`/`ROSE_PP_PIN`/`AOG_SHAPE_CAP`/`RSOLVER_TIMEOUT_MS`（约 10 处） | 恒返回 `Err(NotPresent)` | **无需改**，天然降级到默认值；若 Web 需要开关，改走 JSON 参数 |
| `std::env::args()` | `main.rs` 命令行解析 | 返回空 | **无需改**，WASM 入口不走 `main` |
| `std::fs::File` | `main.rs` 的 `{file}` 参数分支 | 无文件系统 | **删除/绕过**该分支（WASM 只走字符串入口） |
| `std::process::exit` | 错误退出路径 | panic/abort | 改为返回 `Result`/JSON 错误（`io.rs` 已有 `Solution::unsolved` 通道） |
| `std::time::Instant` | **deadline 超时**（`solver/mod.rs`、`dlx.rs`、`backtrack.rs`、`rose/**` 等约 15 处） | **无 OS 单调时钟 → 不可靠** | **唯一必须处理的点**，见 §3 风险#1 |
| `std::sync::OnceLock` / `AtomicUsize` | 缓存 env 检查（`aog_debug_enabled`） | 可用 | 无需改 |
| `HashMap` (`RandomState`) | 多处 | 可用（当前分支 `n1-backtrack-determinism` 已做确定性改造） | 无需改 |
| `serde_json` | I/O 编解码 | 纯 CPU，可用 | 无需改 |

> 补充：`std::env::var` 在 wasm 上「静默返回 None」意味着 `AOG_SHAPE_CAP` 这类**止血开关**在 Web 上失效。
> 如果 Web 要暴露这些开关，最干净的做法是给求解入口加一个 `options: Option<SolveOptions>` JSON 字段，
> 而不是依赖 env。

---

## 3. 关键技术风险与对策

### 风险 #1（最高优先级）：deadline 时钟在 wasm 上不可用

求解器的**防挂死机制**完全依赖 wall-clock 单调时钟 `Instant::now()`（见本仓库记忆：
「挂死根因与 deadline 盲区」——backtrack 曾因缺 deadline 检查挂死）。`wasm32-unknown-unknown`
是「裸」WASM 目标，**没有操作系统，也就没有 `clock_gettime`**，`std::time::Instant::now()`
无法提供真实前进的时钟（行为随 std 版本为 panic 或返回不前进的值）。

**后果**：若直接编译，硬题会无限搜索、烧死标签页。

**对策（推荐 a，成本最低）：**

- **a. 用 `cfg` 抽象时钟**：定义一个 `fn now_monotonic_ms() -> u64`，
  - `#[cfg(not(target_arch = "wasm32"))]` → 走 `Instant::now()`；
  - `#[cfg(target_arch = "wasm32")]` → 走 `web_sys::window().performance().now()`（JS `performance.now()`，毫秒单调）。
  
  所有 deadline 计算点都收敛到「`start` + `Duration`」和「`now() >= deadline`」两个模式，
  封一层即可覆盖约 15 处调用点，改动面可控。

- b. 用 `web-time` / `instant` crate 提供 wasm 版 `Instant`：需替换类型名，改动面略大。
- c. 改目标 `wasm32-wasip1`（WASI 提供 `clock_time_get`）：但 `wasm-bindgen` 不官方支持 WASI，
  JS 互操作变复杂，**不推荐**。

**附带收益**：注入 JS 时钟的同时，可以顺手做**可取消求解**——JS 侧设一个 `AtomicBool` flag，
热循环（`rose_growth.rs` 已有 `steps % 4096`、`dlx.rs` 已有 `search_count & 1023` 这类节流点）
顺带检查该 flag，实现「点停止」比纯超时体验更好。

> ⚠️ 本调研尚未实测 `Instant::now()` 在当前 std（rustc 1.97.1）wasm32 目标上的确切行为
> （`rustup target add wasm32-unknown-unknown` 因网络慢未装完）。**阶段 0 的第一件事就是实证它**，
> 但无论结果是 panic 还是不前进，对策 a 都成立——此风险不改变总体结论。

### 风险 #2：同步阻塞主线程

DFS 是同步紧循环。若在页面主线程跑，硬题（离线口径 30s×3 段）会冻结标签页。

**对策**：放到 **Web Worker**。`wasm-bindgen` 官方支持 worker（`Worker`/`wasm_bindgen_worker`），
worker 内跑 WASM，主线程只做渲染与 `postMessage` 通信。求解接口天然是
`solve(puzzle_json: &str) -> solution_json: String` 的字符串进出，完美适配结构化克隆。

### 风险 #3：正确性保证（「独立校验」不变量）

CLAUDE.md 的核心不变量：**路由器用 `IndependentValidator` 独立复核每个解**，防止有 bug 的求解器走私错误答案。

Web 侧没有 Python `IndependentValidator`。对策：

- Rust 已有 `solver/validate.rs`（完整独立验证器，`build_solution` 已在 pieces/rose/backtrack 路径上强制调用；
  aog 路径走 `build_solution_trusted`，信任 C++ 参考的内部检查）。**Web 直接复用 Rust 侧校验即可**，
  无需移植 Python validator。
- 官方题在 Web 上以**只读浏览 + 求解**呈现（官方解是唯一解，见官方题准则），校验失败会如实上报 `solved:false`。

### 风险 #4：求解时长与体验

离线基准口径是「每段 30s、3 段」；浏览器里这个时长不可接受。对策：Web 默认用**短超时（1–5s）**，
超时如实返回「超时未解」并在 UI 标记；官方题库按 zone 拆包，难题可标注「Web 模式未解」。这是产品取舍，非技术阻塞。

---

## 4. 前端框架选型

用户要求：**「单独写一份 webui」+「使用目前比较流行的前端框架」**。PyQt 层不移植。

### 推荐：React 18 + TypeScript + Vite（默认） 或  Vue 3 + TypeScript + Vite + Pinia

| 维度 | React 18 + TS + Vite | Vue 3 + TS + Vite + Pinia |
|---|---|---|
| 流行度 | 全球最流行，生态最大 | 国内最流行，中文社区成熟 |
| 类型安全 | TS 与 JSON 协议对齐，WASM 绑定类型直出 | 同左（TS 支持好） |
| WASM 集成 | `wasm-pack` 产物 + Vite 对 WASM/Worker 一流支持 | 同左 |
| 静态导出 | `vite build` → 纯静态，GitHub Pages 成熟 | 同左 |

**结论**：两者都满足「流行框架 + 静态托管 + WASM 集成」；团队更熟哪个选哪个。文档后续以 React 为默认描述，
Vue 可等量替换（前端只与「求解 Worker 的 JSON 字符串接口」交互，框架无关）。

备选（说明为何不首选）：
- **Svelte 5**：更轻、写起来更少，但生态/招聘面小于 React/Vue。
- **Yew / Leptos（Rust 前端）**：能复用 Rust 类型、省一层 JS↔Rust 类型桥，但偏离「流行 JS 框架」诉求，且门槛高。
- **复用 `glimmith-solver` 纯 JS UI**：现成，但无框架、需桥两套 JSON 方言，且用户已明确要「流行框架」。

### Web UI 功能范围（对齐 Qt 编辑器核心子集）

1. 画板：格类型（普通 / 数字 / 符号 / 阻断 / compass / fence_pattern / shape_pattern）。
2. 边约束：预画边界、inequality / difference / heterogeneous / homogeneous。
3. 顶点：watchtower。
4. 规则列表：22 条规则参数编辑（对齐 `src/solver/constraints.py` 的 `RULE_CHECKERS` 语义）。
5. 求解交互：调 Worker 求解、进度/取消、`rule_results` 展示。
6. 解渲染：区域着色、区域形状归一化 key、matched shape 名。
7. 官方题库浏览：分区懒加载、点开即解。

---

## 5. 架构方案对比

| 方案 | 描述 | 优点 | 缺点 | 判定 |
|---|---|---|---|---|
| **A（推荐）** | 全新 React/Vue 前端 + `rsolver`→wasm(worker) + 官方题库只读浏览 + GitHub Pages | 完全自主、类型清晰、体验可控 | 前端工作量最大 | ✅ 采用 |
| B | 复用 `glimmith-solver` 浏览器 UI，把它的 JS solver 换成 WASM rsolver | UI 工作量最小 | 无框架（违背用户要求）、两套 JSON 方言需桥、求解器替换侵入其内部架构 | 参考其 UI/规则实现 |
| C | Pyodide（Python in browser） | 复用 Python 模型/校验 | 仍需把 rsolver 编 WASM；Pyodide 体积 10MB+、慢、加载久 | ❌ 否决 |
| D | 复用 `archiveofglimmith.github.io` | 已有静态站点先例 | 只是题库展示，无求解器 | 作浏览层参考 |

---

## 6. JSON 协议与数据

- **协议已单一化**：`src/io/puzzle_codec.py` 定义的 JSON（`grid`/`cells`/`edges`/`vertices`/`rules`/
  `shape_pool`/`outer_boundaries`）即 Rust `io.rs` 反序列化的格式。Web 前端只需按此格式产出 puzzle JSON，
  不需要移植 Python 模型。
- **题库规模**：官方 2488 题 / 17MB（全仓库 2525 个 json）。静态站做法：
  - 打包成单个 `puzzles.json`（约 17MB，gzip 后显著缩小）——参考 archiveofglimmith 的 1.2MB `puzzles.json` 先例；
  - 或按 zone 拆成小 json **懒加载**（首屏更轻，推荐）。
- 求解接口字符串化后，`puzzle_json` 与 `solution_json` 都是纯文本，无需二进制桥。

---

## 7. GitHub Pages 托管

- 纯静态，**完全可行**：`npm run build` 产物 + `.wasm` 产物推到 `gh-pages` 分支（或仓库 `docs/`），
  GitHub Actions 自动部署。
- 注意点：
  1. WASM 的 MIME 需 `application/wasm`——GitHub Pages 默认支持，一般无需额外配置。
  2. 用相对路径 base（`vite.config` 设 `base: './'`）避免子路径 404。
  3. `.wasm` 文件 + `puzzles.json` 走 CDN，无服务器成本；仓库若托管题库，注意 17MB 的 clone 体积（可用浅克隆/单独分支）。
  4. 若不想把题库塞进主仓，可拆成独立 `data` 仓库用 jsDelivr/GitHub Raw 拉取。

---

## 8. 实施路线图（分阶段）

| 阶段 | 内容 | 验收标准 | 量级 |
|---|---|---|---|
| **0 · spike** | 装 `wasm32-unknown-unknown`；最小 `wasm-bindgen` crate 暴露 `solve(str)->str`；**实证 `Instant::now()` 行为**；跑 1 个官方题端到端 | 浏览器控制台拿到正确解 JSON | 0.5d |
| **1 · 时钟+worker** | `cfg` 抽象时钟（`web-time` 或 `performance.now`）；封装 Worker + 可取消 flag | 硬题可超时终止、UI 不卡 | 2d |
| **2 · 前端脚手架+画板** | Vite + React/Vue + TS；网格绘制、格类型/边/顶点编辑、序列化为 puzzle JSON | 能画题并导出合法 JSON 给求解器 | 3–5d |
| **3 · 规则编辑+求解** | 22 规则参数编辑 + 求解交互 + `rule_results` 展示 + 解区域渲染 | 自建题 + 官方题可解可验 | 3–5d |
| **4 · 题库浏览** | 分区懒加载官方题库、点开即解 | 首屏 < 2MB | 1–2d |
| **5 · 部署** | GitHub Actions → GitHub Pages；`base: './'`；WASM MIME 校验 | 线上可访问、可求解 | 0.5d |

> `block` 规则的矩形池前置逻辑（`rust_solver.py::_fitting_rectangles`）建议在**阶段 1/3** 落到 Rust 侧
> （`solver/mod.rs` 的 dispatch 前补一个 `block` 前置），避免 JS 复刻一份并保持单一事实来源。

---

## 9. 工作量估计

前端是绝对大头，合计约 **2 周量级**（1 人）：

- 求解器→WASM 适配（时钟/入口/worker/可取消）：~2–3 人日。
- 前端（画板 + 规则编辑 + 解渲染 + 题库浏览）：~8–12 人日。
- 部署与打磨：~1–2 人日。

求解器本身**不需要算法改动**，这是本项目能快速落地的关键。

---

## 10. 遗留问题 / 待办

1. **阶段 0 实证 `Instant::now()` 在 wasm32-unknown-unknown（std 1.97.1）的确切行为**（本调研网络受限未装完目标）。
2. 官方题「唯一解」保证只适用于官方题库；**用户自建题无唯一解保证**——Web 定位应为「编辑 + 求解 + 约束校验」工具，
   `validate` 只校验「满足约束」而非「唯一」，需在 UI 明示。
3. 是否把 22 规则编辑做全，还是先做「官方题只读浏览 + 求解」MVP（后者工作量减半），待定。
4. `AOG_SHAPE_CAP`/`AOG_ONLY` 等止血开关是否需要在 Web 暴露（建议走 `options` JSON 字段而非 env）。

---

## 11. 实施进度（2026-08-14）

调研结论已落地为代码，全部在隔离 worktree + 分支 `web-wasm-static-site` 上，未触碰主目录。

**已完成的决定（用户确认）：**

- 前端框架：**Vue 3 + TypeScript + Vite + Pinia**（§4 里 React/Vue 二选一，最终选 Vue）。
- PyQt UI 不移植，另写 Web UI（§5 方案 A）。
- **官方解优先**：官方题直接渲染 `Zone*-answer` 的 `regions`（唯一解），**不调用求解器**；
  无官方解的题 / 用户自建题才走 WASM 求解器。实测 `main` 语料 1258 题中 1229 题有官方解、29 题走求解器。

**已落地的代码（`rsolver/` + `web/`）：**

1. `rsolver` 拆为 **lib + bin**：`lib.rs`（暴露 `parse_puzzle` / `solve_json_line` /
   `solution_to_json_text` / `aog_debug_enabled`），`main.rs` 退化为 CLI 薄封装——native 行为不变。
2. **时钟抽象** `clock.rs`：`Instant` 在 native = `std::time::Instant`，在 `wasm32` =
   `performance.now()`（wasm-bindgen 注入）。解决 §3 风险#1（deadline 时钟）。
3. **WASM 绑定** `wasm.rs`：`#[wasm_bindgen] pub fn solve(json: &str) -> String`，默认 5s 超时。
4. 前端骨架：`web/src/worker/`（worker + client）、`lib/`（types + codec）、`store/puzzle.ts`
   （官方解优先逻辑）、`components/`（GridBoard SVG 画板 + PuzzleBrowser 题库）、
   `scripts/bundle-puzzles.mjs`（题库+官方解打包）、`.github/workflows/deploy.yml`（GitHub Pages）。

**验证状态：**

- ✅ `cargo build` + `cargo test`（12+8 通过）+ 原生二进制解 A3-1 端到端——lib/bin 拆分无回归。
- ✅ `bundle-puzzles.mjs` 跑通（1258 题 / 1229 官方解）。
- ⏳ `cargo build --target wasm32-unknown-unknown` + `wasm-pack` + `npm run build` **尚未验证**——
  本机网络受限，`wasm32-unknown-unknown` 目标与 crates.io 依赖未下载完成。代码已就绪，
  网络恢复后按 `web/README.md` 一条命令即可构建（见 §11 遗留）。

**遗留：** ① 网络恢复后跑通 wasm 构建 + `vite build` + GitHub Pages 部署；② 画板交互编辑
（规则/边/顶点）尚未实现（MVP 为只读渲染）；③ compass / fence/shape pattern 的图形化显示待补。
