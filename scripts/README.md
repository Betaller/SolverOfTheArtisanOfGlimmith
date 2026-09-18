# scripts/ 目录说明

本目录存放求解器基准、谜题语料转换、谜题生成等工具脚本。

## 求解与基准

| 脚本 | 作用 | 用法 |
|---|---|---|
| `benchmark_rust_solver.py` | **官方语料基准 / 全量 verify**：对 `puzzles/official/` 全量跑 rsolver，独立验证答案并比对官方解（`matches_official`）。`--timeout` 经 `RSOLVER_TIMEOUT_MS` 真正透传到 Rust 搜索（2026-08-08 修复，原硬编码 30s）。支持两档工作流：<br>• **快速档**（日常回归）：`--baseline <prev.jsonl> --timeout 40 -j 8 --skip-slow` — 用**与基线同口径 timeout** 重跑基线 PASS 题（检 REGRESSION）+ 快 FAIL 题（检 NEW），`--skip-slow` 跳过已知慢题提速；并行负载导致的临界题假回归可用 `--retry-timeouts` 或 solo 复查。exit 2=回归、1=失败、0=干净。<br>• **全量档**（提交前）：`--timeout 40 -j 8 --out <date>_<sha>.jsonl`。<br>另支持 `--resume`、`--zone`、`--rules`、`--adaptive-j`、`--retry-timeouts`（已修三 bug）、`--batch N`（已可精确验证） | `python scripts/benchmark_rust_solver.py --baseline results/bench/latest.jsonl --timeout 40 -j 8 --skip-slow` |
| `compare_batch_ansi.py` | 对比 `batch_run.sh`（C++ AoG_Solver）输出日志与参考 `.ansi` 日志的谜题路径 + 状态序列，用于 C++ 求解器回归 | `python scripts/compare_batch_ansi.py --ref third_party/AoG_Solver/Zone1.ansi --new /tmp/zone1_run.ansi` |
| `solo_eval.py` | **串行单题评估**：一次一题跑 release rsolver（无并行争抢），固定 `RSOLVER_TIMEOUT_MS` 预算，用于把求解器排序/剪枝效果与基准的并行负载噪声隔离开。每题一行 JSON 立即落盘，支持 `--resume` 断点续跑 | `python scripts/solo_eval.py <puzzle_list.txt> <timeout秒> <out.jsonl> [--resume <jsonl>]` |

## 官方语料转换

官方谜题存档 `third_party/archiveofglimmith.github.io/puzzles.json` 是唯一权威源，其余转换都围绕它展开：

| 脚本 | 作用 | 用法 |
|---|---|---|
| `convert_archive.py` | 存档 → 本项目 JSON（`puzzles/official/{Zone1-3}/`），**唯一权威转换器**；含规则/形状/围栏/望塔解析与官方解校验 | `python scripts/convert_archive.py`（`--dry-run` 只校验） |
| `convert_answers.py` | 存档官方解 → 每题独立答案文件（`puzzles/official/{zone}-answer/`），供基准脚本比对 `matches_official` | `python scripts/convert_answers.py` |
| `convert_puzzles_json_to_aog.py` | 存档 → C++ AoG_Solver 的 `.puz` 文件（`aog_puzzles/`），供 `third_party/AoG_Solver/batch_run.sh` 使用 | `python scripts/convert_puzzles_json_to_aog.py` |
| `convert_aog_batch.py` | 借助 `third_party/aog` 的 Rust 解析器批量把 aog 谜题转成本项目 JSON（依赖该解析器已构建） | `python scripts/convert_aog_batch.py` |
| `parse_official_corpus.py` | 解析官方语料 `puzzles.json`，产出结构化的区域/线索数据供统计分析（几何约定：顶点列在字符列 3 的倍数上） | `python scripts/parse_official_corpus.py` |
| `fix_puz_solutions.py` | 把 batch_run 标记为 wrong（或存档无官方解）的 `.puz` 的 SOLUTION 段替换为求解器实际输出，使批量对比通过 | `python scripts/fix_puz_solutions.py --zone Zone1 --batch /tmp/zone1b.ansi --root aog_puzzles` |

## 谜题生成

| 脚本 | 作用 | 用法 |
|---|---|---|
| `gen_ai_puzzles.py` | 生成玫瑰窗 / 形状池 / 组合约束的测试谜题到 `puzzles/aiGen/`，生成前先用 router 解出并验证 | `python scripts/gen_ai_puzzles.py` |
| `generate_polyominoes.py` | 预计算所有自由多连骨牌 1~12 格，写入 `data/polyominoes.json`（供外部工具/未来使用） | `python scripts/generate_polyominoes.py` |

## 开发门禁与实验

| 脚本 | 作用 | 用法 |
|---|---|---|
| `complexity_gate.py` | **圈复杂度门禁**（本项目的 detekt 等价物）：Python 用 radon（阈值 10）、Rust 用 clippy `cognitive_complexity`（阈值在 `rsolver/clippy.toml`）、JS/TS/Vue 用 eslint `complexity`（阈值在 `web/eslint.config.js`）。由 `.git/hooks/pre-push` 与 `.pre-commit-config.yaml` 调用。详见 `docs/code-complexity-report.md` §7 | `python scripts/complexity_gate.py --all` |
| `solver_history.py` | 求解能力历史追踪（CI 趋势图数据管线）：`seed` 从 `docs/official-puzzles-status.md` 里程碑表生成初始 `docs/solver-history.json`；`append` 解析一次基准日志的 `结果: X/Y 通过` 追加数据点；`render` 产出 `docs/solver-history.png` 与 `web/public/trend/index.html` | `python scripts/solver_history.py {seed,append,render}` |
| `exp_shape_cap.py` | 实验：对 21 道 OOM 题分别在三个 `AOG_SHAPE_CAP` 取值下串行跑 rsolver，记录 solved / exit code / elapsed_ms | `python scripts/exp_shape_cap.py` |
| `exp_shape_prescan.py` | 实验：`puzzle_piece` / `mixed` / `different` / `shape_pool` 的静态预扫描传播探针（内部边三态 cut/same/unknown 迭代到不动点），量化每题被强制的比例，调研见 `docs/优化/25-拼块与混合规则静态预扫描传播调研.md` | `python scripts/exp_shape_prescan.py` |

## 约定

- **归档规则**：`benchmark_rust_solver.py` 的输出存 `results/bench/<日期>_<commit-id>_<short-message>.txt`，可执行文件存 `results/bin/rsolver-<commit-id>-<platform>`，临时结果存 `results/tmp/`。详见 `results/README.md` 与 `AGENTS.md`。
- **官方题准则**：官方解是唯一解；转换/校验改动必须用 `convert_archive.py --dry-run` + 全量 verify 验证。
