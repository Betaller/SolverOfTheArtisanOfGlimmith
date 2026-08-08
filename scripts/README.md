# scripts/ 目录说明

本目录存放求解器基准、谜题语料转换、谜题生成等工具脚本。

## 求解与基准

| 脚本 | 作用 | 用法 |
|---|---|---|
| `benchmark_rust_solver.py` | **官方语料基准 / 全量 verify**：对 `puzzles/official/` 全量跑 rsolver，独立验证答案并比对官方解（`matches_official`）。`--timeout` 经 `RSOLVER_TIMEOUT_MS` 真正透传到 Rust 搜索（2026-08-08 修复，原硬编码 30s）。支持两档工作流：<br>• **快速档**（日常回归）：`--baseline <prev.jsonl> --timeout 40 -j 8 --skip-slow` — 用**与基线同口径 timeout** 重跑基线 PASS 题（检 REGRESSION）+ 快 FAIL 题（检 NEW），`--skip-slow` 跳过已知慢题提速；并行负载导致的临界题假回归可用 `--retry-timeouts` 或 solo 复查。exit 2=回归、1=失败、0=干净。<br>• **全量档**（提交前）：`--timeout 40 -j 8 --out <date>_<sha>.jsonl`。<br>另支持 `--resume`、`--zone`、`--rules`、`--adaptive-j`、`--retry-timeouts`（已修三 bug）、`--batch N`（已可精确验证） | `python scripts/benchmark_rust_solver.py --baseline results/bench/latest.jsonl --timeout 40 -j 8 --skip-slow` |
| `compare_batch_ansi.py` | 对比 `batch_run.sh`（C++ AoG_Solver）输出日志与参考 `.ansi` 日志的谜题路径 + 状态序列，用于 C++ 求解器回归 | `python scripts/compare_batch_ansi.py --ref third_party/AoG_Solver/Zone1.ansi --new /tmp/zone1_run.ansi` |

## 官方语料转换

官方谜题存档 `third_party/archiveofglimmith.github.io/puzzles.json` 是唯一权威源，其余转换都围绕它展开：

| 脚本 | 作用 | 用法 |
|---|---|---|
| `convert_archive.py` | 存档 → 本项目 JSON（`puzzles/official/{Zone1-3}/`），**唯一权威转换器**；含规则/形状/围栏/望塔解析与官方解校验 | `python scripts/convert_archive.py`（`--dry-run` 只校验） |
| `convert_answers.py` | 存档官方解 → 每题独立答案文件（`puzzles/official/{zone}-answer/`），供基准脚本比对 `matches_official` | `python scripts/convert_answers.py` |
| `convert_puzzles_json_to_aog.py` | 存档 → C++ AoG_Solver 的 `.puz` 文件（`aog_puzzles/`），供 `third_party/AoG_Solver/batch_run.sh` 使用 | `python scripts/convert_puzzles_json_to_aog.py` |
| `fix_puz_solutions.py` | 把 batch_run 标记为 wrong（或存档无官方解）的 `.puz` 的 SOLUTION 段替换为求解器实际输出，使批量对比通过 | `python scripts/fix_puz_solutions.py --zone Zone1 --batch /tmp/zone1b.ansi --root aog_puzzles` |

## 谜题生成

| 脚本 | 作用 | 用法 |
|---|---|---|
| `gen_ai_puzzles.py` | 生成玫瑰窗 / 形状池 / 组合约束的测试谜题到 `puzzles/aiGen/`，生成前先用 router 解出并验证 | `python scripts/gen_ai_puzzles.py` |
| `generate_polyominoes.py` | 预计算所有自由多连骨牌 1~12 格，写入 `data/polyominoes.json`（供外部工具/未来使用） | `python scripts/generate_polyominoes.py` |

## 约定

- **归档规则**：`benchmark_rust_solver.py` 的输出存 `results/bench/<日期>_<commit-id>_<short-message>.txt`，可执行文件存 `results/bin/rsolver-<commit-id>-<platform>`，临时结果存 `results/tmp/`。详见 `results/README.md` 与 `AGENTS.md`。
- **官方题准则**：官方解是唯一解；转换/校验改动必须用 `convert_archive.py --dry-run` + 全量 verify 验证。
