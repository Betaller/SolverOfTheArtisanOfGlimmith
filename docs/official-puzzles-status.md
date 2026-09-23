# 官方谜题求解状态

> **准则：官方题的官方解是唯一解。**
> 对求解器 / 转换脚本 / 规则校验器 / 规则语义的**每次优化**，必须在本文件**第一部分（进度）与第二部分（变更）各追加一条**，并同步相关文档、跑测试（见文末「软门禁」）。

---

## 第一部分：进度

> 全量扫描 / 基准快照，按时间顺序**往下追加**（旧的在上）。每题完整求解结果存 `results/bench/`（基准）或 `results/tmp/`（verify / 临时），随提交入库。下表「通过」= 求解 + 独立验证均通过的题数 / 总数；「较上次」以同口径上一基准为参照。

| 日期 | 里程碑（commit） | 结果文件 | 工具 | 通过 | 较上次 | 备注 |
|---|---|---|---|---|---|---|
| 2026-08-05 | 修复后基准（`33d32c5`） | `results/bench/20260805_33d32c5_rust-official-bench.txt` | `benchmark_rust_solver.py --timeout 20 -j 8` | **1040 / 1258** | 基准 | Zone1 300/312 · Zone2 387/438 · Zone3 329/481；失败全为超时。修复前 verify ~190 FAIL 大量为 gemini/delta、玫瑰窗、环纹 bug 的错解被接受；本次失败均为「解不出」而非「错解」。 |
| 2026-08-05 | rose 求解器下沉 Rust（`4733f59`） | `results/tmp/20260805_4733f59_rose-port-rust-only.txt` | `benchmark_rust_solver.py --timeout 25 -j 8`（router 只走 RustSolver） | **1048 / 1258** | +8 | 纯 rose_window（C4-1 / 0277 / 0213 / 0213nopad）新解出且与官方一致；0833（10×11）时解时不。注：router 仍保留 Python 兜底（Rust-only 有 2 题解不出：1301/0957）。 |
| 2026-08-06 | rose 尺寸感知优化（`7e569e7`） | `results/bench/20260806_7e569e7_rose-size-aware-fix.txt` | `benchmark_rust_solver.py --timeout 25 -j 8` | **1047 / 1258** | 较基准 +7 | Zone1 300/312(0) · Zone2 393/438(**+6**) · Zone3 328/481(-1)。提升：**range+rose**（1334/1342 由 30s FAIL → <1s 解出）。Zone3 -1 为 aog 预算下调后的计时/非确定性波动。 |
| 2026-08-06 | brick 回溯短板闭合（本会话） | `results/bench/20260806_dfadfe3_brick-gap-rust-only-bench.txt` | `benchmark_rust_solver.py --timeout 40 -j 8` | **1052 / 1258** | +5 | A/B/C 26/27(0) · Zone1 301/312(+1) · Zone2 395/438(+2) · Zone3 330/481(+2)，**0 真实新增失败**。新解出：1301、0957（brick+block+rose ≈1.9s）、0732/0710/0795/0265/1382；1301/0957 缺口全部闭合。0957/0985 在全量并行下偶发 exit -9 / 超时（solo 均解出，负载波动非回归）。 |
| 2026-08-06 | Python 求解器移除后 Rust-only router 验证 | `results/tmp/20260806_rustonly-router-verify-zone1.txt` | `benchmark_rust_solver.py --dir puzzles/official/Zone1 --timeout 25 -j 8`（`default_router` 只走 RustSolver） | Zone1 **301 / 312** | 与 dfadfe3 基准 Zone1 完全一致 | 移除 Python 兜底**零回归**。11 失败：0882 exit -9（并行内存压力）；0223/1435 错解被 IndependentValidator 拦截；其余为已知超时/UNSOLVED。`pytest` 290 通过、`cargo test` 9 通过。 |
| 2026-08-06 | 删除 constraints.rs stub（fence/compass/ring 信任缺口修复） | `results/tmp/20260806_82c9132_verify-full.txt` | 1295 题全量 verify 基线 | 1295 − 228 = **1067 通过 / 228 失败** | — | 删除 9 条恒 `true` 的 stub，`build_solution` 与 pieces 改用 `solver/validate::validate` 全量复查。30 题「答案未通过独立验证」→ Rust 内**诚实拒绝**（不再上报错解）。36/36 抽样与 `*-answer` 官方解一致，0 个「合法但不同」；40 抽样 + 10 ring/compass PASS 题 **0 回归**。脚本新增 `matches_official` 比对（DIFF 即失败）。 |
| 2026-08-06 | 边界望塔修复（watchtower 顶点绝对坐标约定） | `results/tmp/20260806_f1cfa16_watchtower-verify.txt`（专项）+ `results/tmp/20260806_f1cfa16_final-verify.txt`（全量）；二进制 `results/bin/rsolver-f1cfa16-linux-x86_64` | `benchmark_rust_solver.py` | **1070 PASS / 225 FAIL / 0 DIFF** | vs 基线 1067/228/7DIFF，净 **+3 PASS** | 顶点约定改绝对网格坐标 `(0..=h × 0..=w)`，转换器收集全部边界望塔，85 个 watchtower JSON 迁移 vertices。**watchtower DIFF 全部消除（0 DIFF）**，6 道（0543/0544/0662/0663/0800/1144）与官方解一致；专项 50 PASS / 35 FAIL **0 回归**；14 个 PASS→FAIL 均为并行负载临界波动（单跑解出）。 |
| 2026-08-07 | 搜索前边界推演 + 中搜索形状剪枝 + BF 默认开启（`6169df3`） | `results/bench/20260807_c6cb307_opt-v3-bench.txt` | `benchmark_rust_solver.py --timeout 40 -j 8` | **1046 / 1258** | 较上一进度（1052）波动 -6 | A/B/C 26/27(0) · Zone1 300/312(-1) · Zone2 393/438(-2) · Zone3 327/481(-3)。与基线（`results/bench/20260807_231d8d2_sat-only-bench.txt`）共同 1074 题逐题对比：**0 PASS→FAIL，5 FAIL→PASS**（1270/0749/1329/0875/0795），**无算法回退**；Zone 波动属跨运行临界题在 40s 边界摇摆 + 前轮僵尸进程 CPU 争抢。提升：约束边→边界穿透 + 密封区域即时剪枝 + BF 面积传播默认开启 + ring/brick 预检（0 panic）。212 FAIL = 117 无解 + 73 超时 + 14 OOM + 8 校验失败。脚本新增 `--retry-timeouts`（有 bug 待修）。 |
| 2026-08-07 | fence 规则搜索中增量剪枝（专用求解器第一波 #1，`cd40cab`） | `results/bench/20260807_cd40cab_fence-midsearch.txt` | `benchmark_rust_solver.py --timeout 40 -j 8` | **1047 / 1258** | +1 | 新增 `solver/fence/` 模块（独立文件夹，仿 `rose/`）：`FenceCellData` 预计算每个 fence 格的 `arm_count`（dihedral 不变量）与 `pattern_dihedral_key`；`check_fence_patterns` 作为无状态守卫挂入 backtrack `dfs` 守卫链（仿 `check_sealed_regions`），`has_fence` 门控让 1046 非 fence 题零开销。核心：4 边界位全定时做 dihedral_key 比对；未全定时用 arm-count 部分检查（`T>k` 或 `F>4-k` 即剪）。与 `c6cb307` 基线逐题对比：**0 回归**，新增 PASS **0829**；**8 道校验失败 → 0**（backtrack 不再产出 fence 错解，失败模式转无解/超时/OOM——正确性修复）。剪枝实测生效（0401：131611 次剪枝 / 167422 步）。fence 子集 171 题 PASS 数未变（127→127），搜索空间仍太大，后续拟叠加边界预推导 + NonBoundary DSU 合并（见 `docs/优化/10-专用求解器方案.md` §B.2）。 |
| 2026-08-08 | rose 解除 puzzle_piece 禁令 + 预钉 shape_pattern 区域 + timeout 透传修复（`rose-pp-pin`） | `results/bench/20260808_bd2f5f5_rose-pp-pin.jsonl` | `benchmark_rust_solver.py --timeout 40 -j 8` | **1052 / 1258** | +5 | 新增 `solver/rose/puzzle_piece_pin.rs`：枚举每个 shape_pattern 格的 dihedral 变体合法放置 + 符号约束过滤 + 多锚点笛卡尔积。`solve_rose` 加预钉分支：缩减 all_positions + m' → region_match → 合并预钉区域 → accept_if_valid；m'=1 快速路径（剩余格单连通分量直接成区域，避开候选截断）。解除 `region_match.rs:285-291` 的 puzzle_piece/shape_pool 硬禁令；修复 region_match 种子收集（seeds/all_seed_cells 改为只从 all_positions 收集）。**配套 timeout 透传修复**：`main.rs::resolve_timeout_ms` 读 `RSOLVER_TIMEOUT_MS` env var（原 `solve_json_line` 硬编码 30s，`--timeout 40` 到不了 Rust），`RustSolver._subprocess_env` thread 入；移除 rose 的 30s clamp。与 `cd40cab` 基线逐题对比：**0 回归**，新解出 5 道——**0732**（puzzle_piece+rose_window，via rose 3002ms，rose-pp-pin 直接收益）；0685/0710/1320/1348（aog 临界题，timeout 修复后拿满 40s 解出）。fence 预推导 DSU 方向同期证伪（见 `docs/优化/10-专用求解器方案.md` §3.3 警示框），fence-anchor-bfs 分支未合 main。 |
| 2026-08-08 | timeout 透传修复 + rose clamp 移除（求解能力变化） | （全量待补，见下） | `benchmark_rust_solver.py --timeout 40 -j 8 --out results/bench/<date>_<sha>.jsonl` | **待全量验证** | — | `main.rs:86`/`io.rs:solve_json_line` 硬编码 `30_000` → `resolve_timeout_ms()` 读 `RSOLVER_TIMEOUT_MS`，`RustSolver` 从 `--timeout` 设入；`solver/mod.rs:83` 移除 `ROSE_TIMEOUT_MS=30_000` 的 `.min()` clamp。**求解能力变化**：`--timeout 40` 此前对 Rust 完全无效（固定 30s），现真正给 aog/pieces/backtrack 各 40s、rose 最多 40s（原 clamp 30s）→ 原 30s 临界 FAIL 的题（尤其 rose-capable Zone3 慢题）可能在 40s 内新解出。**预期 NEW>0、REGRESSION=0**（纯 timeout 修复，无算法改动）。全量基准待跑后回填通过数；快速档可用 `--baseline latest.jsonl --timeout 40 -j 8 --skip-slow`（同口径 timeout，跳过已知慢题）做日常回归。`benchmark_rust_solver.py` 同步新增 `--baseline`/`--zone`/`--skip-slow`/`--skip-slow-threshold`、修复 `--retry-timeouts` 三 bug。 |
| 2026-08-08 | aog 形状库硬上限（第一波 #1，`shape-cap-aog`） | `results/tmp/20260808_shapecap_default0_regression.jsonl`（cap=0 全量回归）+ `results/tmp/20260808_shapecap_experiment.json`（21 OOM × 3 档 cap 实验） | `benchmark_rust_solver.py --timeout 40 -j 8 --baseline latest.jsonl`（cap=0）+ 直跑二进制 21 OOM 扫描（`scripts/exp_shape_cap.py`，cap=50k/100k/200k） | **cap=0：1052 / 1258 口径不变**（详见备注） | — | 新增 `AoGCore::shape_cap: usize` 字段（`core.rs`）+ `DEFAULT_SHAPE_CAP = 0`（`types.rs`，默认关闭）+ `AOG_SHAPE_CAP` env var。`shapes_insert`（`core.rs:169`）顶部守卫：库满则原子拒绝全部 8 个 dihedral 变体（`return 0`，**不在 `add_shape_to_shapes` 内部做**——否则部分插入破坏对称性）。调用点 `search.rs:248` 加 `if shape_index == NO_SHAPE_INDEX { continue; }`——防止 `NO_SHAPE_INDEX (0xffff)` 写入 `sp` 后在 `shape_size_by_index[65535]` 越界 panic（exit 101，6 处索引点：`core.rs:243/251/281/282`、`empty.rs:403`、`search.rs:1332`）。`predefine_shapes_only`（shape_pool 规则）天然豁免（其 DFS 不调 `shapes_insert`，`search.rs:1289` 已短路）。**cap=0 全量回归**：1 REGRESSION（0685，38441ms→40s 超时，临界负载波动非算法回归，与 c6cb307 基准的 14 道 PASS→FAIL 同性质）+ 1 NEW（0957，OOM→2021ms PASS，已知负载不稳题）。**0 算法回归、0 panic**（cap=0 时 `shape_cap>0` 守卫短路，行为与基准逐字节一致）。**21 OOM 三档实验**（50k/100k/200k）：**16/21 由 exit -9（OOM）转为 exit 0（优雅超时）**——cap 成功止血 aog OOM；3 档结果一致（cap 值不敏感，50k 已够）；**仅新增 1 PASS（0957）**——止血≠解出，多数转为 40s 超时；**3 道 rose_window OOM（0882/0826/0838）+ 1 道 0999（rose+watchtower）不受影响**（OOM 在 rose solver 的 `region_match` visited HashSet，非 aog 形状库——需第一波 #3 rose visited 上限）；**2 道（0606/1215）80s 退出**（40s deadline 在 capped 库上未及时触发，非致命，后续可加 `shape_cap_exhausted` 紧 deadline）。`cargo test` 18 通过（含新增 `test_shape_cap_refuses_new_shapes`/`test_shape_cap_zero_unlimit`）；`pytest` 290 通过。详见 `docs/优化/12-优化项价值评估与路线图修订.md` §3.2 #3、`docs/rust-solver/04-aog求解器.md`。 |
| 2026-08-08 | rose visited 硬上限 + rose_growth deadline 修复（第一波 #3，`rose-visited-cap`） | `results/tmp/20260808_rose-2m-quick.jsonl`（快速回归）+ 4 rose OOM 专项直跑 | `benchmark_rust_solver.py --baseline latest.jsonl --timeout 40 -j 8 --skip-slow` + 直跑二进制 4 OOM 扫描 | **快速回归口径不变**（详见备注） | — | 两项改动：① `region_match.rs` 加 `VISITED_CAP = 2_000_000`（`visited` HashSet 硬上限，bail-out 返回部分 results 防 OOM）；② `rose_growth.rs` 修复 `_deadline` 未用 bug——`solve_singlesymbol`/`solve_multisymbol` 加 deadline 检查（wavefront 每 4096 步、second-pass 每 64 轮、入口），fallback 不再挂死。**#2 compass 边界框预推导经对抗审查证伪弃做**（bbox 证明"框外格不在 compass 区域"但不能证明"框内格就在 compass 区域"——框内 P 与框外 Q 同属另一区域时强制 is_boundary 破坏合法解；0630 等 13/15 目标题误强边致不可解）。**实测**：0999（rose+watchtower 14×14）OOM→exit 0 止血 ✓；0833（rose_window 10×11）200k cap 时回归（部分候选丢真解→rose_growth 挂死），**2M cap + rose_growth deadline 修复后重新 PASS（9482ms）** ✓；0882/0826/0838 仍 OOM（根因在 `enum_area_combos_bounded` 无界组合枚举，非 visited——另题）。**快速回归 5 REGRESSION 全是 aog 预存非确定性挂死**（0749/0829/0875 的 "LB: sealed" 循环、1329/0795 临界负载波动；0749 在 main 二进制也挂、0829/0875 solo 也挂——均非 rose 改动所致）。0 rose 回归、0 panic。`cargo test` 20、`pytest` 290 通过。详见 `docs/rust-solver/07-rose求解器.md`、`docs/优化/12` §3.2 #2(证伪)/#6。 |
| 2026-08-14 | edge_csp 边变量 CSP 求解器第一迭代（`edge-csp-solver`） | `results/tmp/20260814_edgecsp-full.jsonl`（全量） | `benchmark_rust_solver.py --timeout 40 -j 8` | **1072 / 1258** | 较 bd2f5f5 基线净 **+20**（14 edge_csp + 7 前序 aog 修复 − 1 flake） | 新增 `solver/edge_csp/`（边变量 CSP，见 `docs/rust-solver/11-edge-csp求解器.md`）：三态边（`Unknown`/`Cut`/`Uncut`）+ 不动点传播（顶点度/面积界/线索）+ failed-literal 探测 + 边 DFS，输出经 `validate::validate` 复查。覆盖 ring/brick/area/precise/range/inequality/difference。**14 道新解出（全过独立验证，solver=edge_csp）**：0421/0507/0592/0637/0638/0894/0979/1131/1132/1134/1382/1400/1404/1411（difference/inequality/ring 系）。**关键正确性修复**：`propagate_bricky_loopy` 数度含**外边框与 blocked 格边**（参考 aog 只数内部边且不叶验 ring/brick，会产边界 T 型错解被 validate 拒——0666 等）。**路由**：后置 fallback（aog/rose 之后、pieces 之前），`is_edge_csp_capable` 排他门控（所有规则 ⊆ {ring,brick,watchtower,compass,inequality,difference,area,precise,range}）；`RustSolver.RUST_PARTS` 3→4。**未做（迭代二）**：compass 方向计数 / watchtower / differentiation 传播、ring OOM 前置拦截。**1333（rose+range，无 edge 规则）PASS→FAIL 与 edge_csp 无关**（`is_edge_csp_capable` 不触发，flake）。`cargo test` 20、`pytest` 290 通过。 |
| 2026-08-14 | edge_csp 第二迭代：fence + compass 基础 + 内部验证（`edge-csp-iter2`） | `results/tmp/20260814_edgecsp-iter2-full.jsonl`（全量） | `benchmark_rust_solver.py --timeout 40 -j 8` | **1076 / 1258** | 较第一迭代净 **+4**（4 fence − 1 flake） | ① **叶节点内部验证**（P0）：`Solver` 存 `&'a Puzzle`，`backtrack_edges` 在 `curr_unknown==0` 时 `extract_regions` 后先 `validate::validate` 通过才 save、否则继续回溯——是 fence/compass 的前置（部分传播会产中间无效解）。② **compass 方向计数基础**（P1）：`propagate_compass`（0 方向邻边强制 Cut）+ `propagate_compass_in_components`（方向计数/到限 Cut/缺限单网关 Uncut/两两相容/边界框剪生长边）——小 compass 题正确，大 compass 题仍需桥/网关强制（迭代三）。③ **fence 围栏**（P2）：`palisade_kind`（3×3 十字→`PalisadeKind`）+ `propagate_palisade_constraints`（4 旋转枚举取交集强制边），`SUPPORTED` 加 `fence`。**4 道新解出（solver=edge_csp）**：0628/0903/0923fix/0924fix。**1131（area+difference+inequality）PASS→FAIL 为 flaky**（edge_csp 在 27-60s 边界，solo 仍解出，非算法回归）。`cargo test` 20、`pytest` 290 通过。 |
| 2026-08-14 | edge_csp 第三迭代：compass 桥/网关（`edge-csp-iter3`） | `results/tmp/20260814_edgecsp-iter3b-full.jsonl`（含回退前 shape cap 试验） | `benchmark_rust_solver.py --timeout 40 -j 8` + 0621 直跑 | **1076 + 0621**（详见备注） | +1（0621） | ① **compass 桥/网关强制**：`force_compass_via_bridges_and_gateways`（可达子图 + Tarjan 桥 + 单网关边强制 Uncut）+ `find_bridges_in_subgraph`（迭代 Tarjan，~300 行）。新解出 **0621**（compass+difference ~3s，was 40s FAIL）。② **OOM 止血调研（已回退，净负）**：`DEFAULT_SHAPE_CAP` 0→50k 试验——16/21 OOM→优雅超时，但**回归 ~12 道 aog 题**（搜索合法超 50k 库条目），默认回 0（注释存档）；`is_edge_csp_preempt` 细化但不接入（cap 开时冗余、关时 moot，只会小块题重归因）。**结论：OOM 止血需 deadline 触发式 cap 等精准手段，50k 一刀切不可取**（`docs/rust-solver/11` §8）。`cargo test` 20、`pytest` 290 通过。 |
| 2026-08-17 | edge_csp 第四迭代：watchtower 顶点传播（`edge-csp-watchtower`） | `results/tmp/20260817_watchtower-r1.jsonl`（85 题）+ `results/tmp/20260817_watchtower-regquick.jsonl`（1148 回归快扫）+ `results/bin/rsolver-d5082a9-linux-x86_64` | `benchmark_rust_solver.py --rules watchtower --timeout 40 -j 8` + `--baseline --skip-slow` 快扫 | **1080 / 1258**（估算：1075 + 4 edge_csp 新解 + 1 iter3 0621 已计；详见备注） | +4 via edge_csp（0405/0419/0983/1140fix） | 移植参考 `third_party/aog watchtower.rs::propagate_watchtower` 进 edge_csp 不动点循环（`!vertex_clues.is_empty()` 门控，area_bounds 之后）。两遍：① Pass A（component-ID）数顶点 2×2 distinct sealed/growing 组件→`[min,max]` 区间，越界矛盾、`max==value` 时强 Cut 不同组件间 Unknown 边；② Pass B（edge-count）按 cycle/tree 拓扑（`pieces=max(1,k)` / `1+k`）与 value 推 Cut/Uncut，处理 double-touching。另加 value==1 启动优化（内部顶点 4 格全在时预强 4 内部边 Uncut）。`VertexClue.value` 不再 dead_code；`grid::vertex_pos` 逆映射新增。**4 道新解出（solver=edge_csp，均过 validate + 匹配官方）**：0405/0419/0983/1140fix（was 40-80s 超时，现 40-70s 内解出）。**0 真回归**：1148 baseline-PASS 快扫 0 回归；3 个"假回归"（1270/0749/0875，均**无 watchtower**、propagator 不触发）经隔离重跑确认仍解出（aog 抖动/系统噪声）。另 7 道 aog 抖动新解（0384/0504/0658/0710/1318/1381/1405）非 propagator 功劳。`cargo test` 20、`pytest` 290 通过。 |
| 2026-08-17 | edge_csp 第五迭代：watchtower parity 传播（`edge-csp-watchtower-parity`） | `results/tmp/20260817_parity-watchtower.jsonl`（85 题）+ `results/tmp/20260817_parity-regquick.jsonl`（1148 回归快扫）+ `results/bin/rsolver-04ea400-linux-x86_64` | `benchmark_rust_solver.py --rules watchtower --timeout 40 -j 8` + `--baseline --skip-slow` 快扫 | **1081 / 1258**（1080 + 1 net，详见备注） | +2 via edge_csp（0983/1000） | 移植参考 `watchtower.rs::propagate_vertex_edge_parity`（330-533）进不动点循环（`!vertex_clues.is_empty()` 门控，bricky_loopy 之后）。新增独立 `parity_uf.rs`（移植 `third_party/aog/src/uf.rs`，ParityUF XOR 并查集）。对 cut-count 奇偶确定的顶点（cycle 仅 value==4、tree value∈{1,3,4}）建未知内部边间成对 XOR 约束，全局经 UF 传播（三阶段：0/1/2 未知→校验/强制/union；3+ 未知用 UF 对约简；已知值级联解未知）。另移植 `probe_watchtower_vertex_configs`（顶点配置枚举探测）但**禁用为 dead code**——实测较 parity 0 增量解（parity UF 已捕获可强制边），保留供未来 compass+watchtower 题用。**2 道新解出（solver=edge_csp，均过 validate + 匹配官方）**：0983（was 80s 超时）、1000（fence+watchtower，was 40s 超时）。**0 真回归**：2 假回归（0749/1270 无 watchtower + 隔离重跑仍解）。`cargo test` 20、`pytest` 290 通过。 |
| 2026-08-17 | edge_csp 第六迭代：compass 放置枚举（`edge-csp-compass-placement`） | `results/tmp/20260817_compass-placement.jsonl`（129 题）+ `results/tmp/20260817_compass-regquick.jsonl`（1148 回归快扫）+ `results/bin/rsolver-d3d9c52-linux-x86_64` | `benchmark_rust_solver.py --rules compass --timeout 40 -j 8` + `--baseline --skip-slow` 快扫 | **1082 / 1258**（1081 + 1） | +1 via edge_csp（0445） | 移植参考 `area.rs::propagate_compass_placement_enumeration`（1742-2124）+ `compass_placement_dfs`（2129-2213）进 `propagate_compass_in_components` 之后。对 `max_area≤12` 的 compass 线索：BFS bbox 内可达格 → **全局 Uncut flood-fill** 建局部组件（跨 bbox 跟 Uncut，已 committed 外部格拖整片防误强 Uncut）→ DFS 枚举合法连通合并（include/exclude 最小索引优先，方向精确 + 尺寸界，cap 12/16/500）→ `in_all` 强 Uncut / `in_any` 无+bbox外强 Cut。自门控 `in_probing`。**1 道新解出（solver=edge_csp，过 validate + 匹配官方）**：0445（11×11 纯 compass 25 格，was OOM exit -9 → 73s 解出；placement 强制边大幅缩搜索空间止血 OOM）。1140fix 也解出但是 iter4 已解的 flaky 临界题（非本迭代可靠收益）。**0 真回归**：1148 快扫 0 回归。`cargo test` 20、`pytest` 290 通过。 |
| 2026-08-17 | edge_csp 第七迭代：differentiation + boxy/non_boxy 传播（`edge-csp-differentiation-boxy`） | `results/tmp/20260817_diff-diff.jsonl` + `results/tmp/20260817_diff-nonblock.jsonl` + `results/tmp/20260817_diff-regquick.jsonl`（1148 回归快扫）+ `results/bin/rsolver-150005f-linux-x86_64` | `benchmark_rust_solver.py --rules differentiation/non_block --timeout 40 -j 8` + `--baseline --skip-slow` 快扫 | **1085 / 1258**（1082 + 3） | +3 via edge_csp（0690/0971/0984） | 移植参考 `area.rs::propagate_size_separation`（370-485，differentiation）+ `propagate_boxy_nonboxy`（904-1001，block/non_block）+ sealed-pair 检查（1244-1265）进 `propagate_area_constraints`。size_separation：建 sealed_neighbor_sizes→合并边面积等邻居强 Cut/禁面积仅 1 生长边强 Uncut；boxy/non_boxy：bbox 填充判定矩形，sealed 矛盾→Err、growing 不可填(boxy)→Err、1 可填洞(non_boxy)→强 Cut 防矩形。`GlobalRules` 加 `boxy`/`non_boxy`；SUPPORTED 加 `differentiation`/`block`/`non_block`。**3 道新解出（solver=edge_csp，均 non_block+companion，过 validate + 匹配官方）**：0690/0971/0984（was 40s 超时）。differentiation 0 新解（propagator sound 但目标题未破）；boxy(block) 0 目标题（FAIL block 题均带 rose/different/solitary 被排他）。**0 真回归**：2 假回归（0265/0875 有 differentiation 但 0875 带 solitary 不触发 edge_csp、0265 隔离重跑仍解 18s）。`cargo test` 20、`pytest` 290 通过。 |
| 2026-08-17 | 求解器返回信息完善：attempts 求解链 + ModuleOutcome（`9d8461f`，可观测性，非算法） | `results/bench/20260817_9d8461f_attempts-full.jsonl`（全量）+ `results/bin/rsolver-9d8461f-linux-x86_64` | `benchmark_rust_solver.py --timeout 30 -j 6` | **1084 / 1258** | 较 iter7 估算 1085，**−1（aog 抖动，非算法回归）** | **可观测性改动，求解能力不变**。`Solution` 加 `attempts: Vec<SolverAttempt>`（per-module 求解链，doc 23）：每个分派考虑过的模块一条 `{solver, status, elapsed_ms, note}`，`status` 六态 `success`/`timeout`/`exhausted`/`validation_failed`/`not_attempted`/`error`。五个 `solve_*` 返回类型 `Option<Vec<RegionInfo>>` → `ModuleOutcome`（`Solved`/`ValidationFailed`/`None`），把 aog/edge_csp/rose 内部「找到候选但 validate 拒绝」从 `None` 里分出来（原先被吞掉，对 FAIL 根因分析最关键）。`mod.rs` 分派处包裹计时+记录；`build_solution` 校验失败时把刚 push 的 success 改写成 validation_failed。JSON `attempts` 为新增可选字段（`skip_serializing_if=Vec::is_empty`，旧消费者不解析即忽略）。Python 侧 `SolverAttempt` 统一到 `models/solution.py`（L1 router 链 + L2 Rust 模块链共用，带 `AttemptStatus` 枚举 + `solved`/`error` 兼容 property）；UI 结果面板加求解链表格；benchmark `via=` 显示完整链（如 `aog:timeout→rose:success`）+ JSONL 落 `attempts`。**端到端实测**：C4-1 `aog:timeout(3011ms)→rose:success(54ms)`（旧 JSON 只说 solver=rose）；1169 `aog:validation_failed→rose/edge_csp:not_attempted→pieces:success`（validation_failed 成功浮出）；0839/1406 `aog:timeout→rose:validation_failed→backtrack:exhausted`（rose 产假解被 validate 拒，可见）。**0 真回归**：1 道差异 1270（homogeneous+rose_window，solo timeout=40 1.9s 解出 aog:success）为已知 aog 非确定性抖动题（iter4 已记录 1270/0749/0875 均为 aog 抖动假回归），与 attempts 改动无关。**已知边界**：OOM/外部 kill 时进程没机会写 JSON，`attempts=[]`（如 0882/0826/0838 rose OOM 题）；timeout/exhausted 用 `now>=deadline` 近似判定，精确 deadline-hit 信号留待 deadline 盲区整改（doc 17）。`cargo test` 20、`pytest` 全绿、ruff 改动文件错误数 19→16（未引入新错误）。详见 `docs/优化/23-求解器返回信息完善方案.md`。 |
| 2026-09-02 | 1102 收口基准（`c10e181`） | `results/bench/20260902_c10e181_verify.jsonl` | `benchmark_rust_solver.py --timeout 40 -j 8` | **1102 / 1258** | +18（vs 1084） | 1120 里程碑前的全量收口基准（rose-cap 预算 + rose deadline 锚定之前）；逐题解 + 独立验证均过，新解构成见下方 1120 条目。 |
| 2026-09-09 | 1120 里程碑（PR#64 `a04a4ad` = 7c05d41 + de1f3f4） | `results/bench/20260909_areafeas_full.jsonl` | `benchmark_rust_solver.py --timeout 40 -j 8` | **1120 / 1258** | +18（vs 1102） | ① `AOG_ROSE_BUDGET_MS` 3s→20s：31 道 rose-capable 题原被 aog 3s 截断全超时，放宽后 aog 解 0213/0213nopad/0856/0957/0620/1386；② `solve_rose` 把 deadline 锚定自身起点（修复 router 全局 start 致 rose 剩余预算为负、0ms 返回，曾让 30s aog 预算回退 6 道）；③ 边 CSP `propagate` 加声音的面积可行性裁剪（两组件合并区最小尺寸已超任一方 max_area 则强 Cut，剪 0289 等爆炸，0 真回归）。新解 0439/0491/0445/0651 via edge_csp。1120/1258，0 真回归。 |
| 2026-09-18 | edge_csp 形状同一性传播（same/different/mixed，`feat/shape-identity-propagation`） | `results/tmp/20260918_shape-identity-verify.txt`（定向验证；全量基准待合入前跑） | 直跑 rsolver + 219 题形状规则回归 | **1123 预估 / 1258**（+3，全量待确认） | +3（vs 1120） | `GlobalRules` 加 mingle/mismatch/mixed 三标志；`propagate_shape_constraints` 扩展 `check_mingle`（全局同形：首个密封组件定尺寸 a 后，超尺寸/目标≠a/潜力<a 判矛盾，达 a 强制封口）、`check_mismatch`（密封形状两两互异，BTreeSet）、`check_mixed`（Cut 边两侧密封同形判矛盾）。`is_edge_csp_capable` 放开纯形状同一性题（rose_window+same 等——`is_rose_capable` 拒绝它们，此前仅 aog 尝试）。**新解 0341/1370（different+fence）、1340（different+rose_window）via edge_csp**；219 道形状规则 PASS 题 0 回归；select_edge 启发式扩展（clue 约束/Slitherlink 端点/rose 邻近）实测致 0924fix/0972 真回归已回退。`pytest` 301、`cargo test` 34 通过。 |
| 2026-09-18 | compass bbox 面积界回填 + solitary 可行集 + pieces compass deadline | `results/tmp/20260918_compass-bbox-verify.txt`（定向验证） | 直跑 rsolver + 154 题 compass/solitary 回归 | **1126 预估 / 1258**（+3，全量待确认） | +3（vs 1123） | `get_compass_area_bounds` 对含 `-1` 方向的罗盘线索给出 `max = 1+Σ(v_d or 半平面可存格数)`（原先四方向全已知才有 max），激活 `size==max→封口`/`growth_potential`/放置枚举门槛（够格线索 58→155）；`propagate_solitary` 加 S5 bbox 可行集（S5a/b/c，仅全罗盘线索且 K≤64 激活）；`pieces::compass_rec` 加 deadline（0312/0680 不再被 harness 击杀）。**新解 1386（compass+rose）、0418/1140fix（compass+watchtower）via edge_csp**；154 道 compass/solitary PASS 题 0 回归；compass+solitary FAIL 簇 11 题仍未解（S5b 根层不触发）。 |
| 2026-09-18 | edge_csp dual_connectivity D1/D2（structural_pieces via precise） | 直跑 rsolver | 直跑 rsolver + 169 题 precise PASS 回归 | **1128 预估 / 1258**（+2，全量待确认） | +2（vs 1126） | 新字段 `structural_pieces`：仅从 `precise` 规则推导（fillable/A 整除时），刻意不用玫瑰窗计数（后者会打开 two-piece parity seeding 的错误强制，见 doc 27）。`propagate_dual_connectivity`：D1 须生长而仅 1 条 Unknown 生长边→强制 Uncut；D2 组件图连通分量数 >片数→矛盾、==片数→分量内 Unknown 边全 Uncut。**新解 0209（precise+ring，15.7s）、0703（fence+precise，1.8s）via edge_csp**；169 道 precise PASS 题 0 回归；1248（fence+homogeneous+precise）仍超时。 |
| 2026-09-18 | edge_csp 卫生项打包（structural_pieces_max / compass 不兼容 / S5d / rose 源） | 直跑 rsolver | 定向回归 | **1128 预估 / 1258** | 0（均为 0 增益 0 回归的声音基础设施） | ① `structural_pieces_max`（range/precise 的 min_area 派生片数上界）供 D2 的 `cc > cap` 矛盾用；② `init_compass_incompatibility`（相邻不相容罗盘对预强制 Cut）；③ solitary S5d（单候选格的唯一邻接强制 Uncut）；④ `structural_pieces` 玫瑰窗源。四项各自实测 0 新解，但回归全绿（分别 273 / 96 / 154 / 56 题），保留为后续优化的地基。 |
| 2026-09-18 | **全量收口基准（`65d2336`）** | `results/bench/20260918_65d2336_shape-identity-compass-dual.{txt,jsonl}` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1127 / 1258** | **+7**（vs 1120） | 9 新解 / 2 损失。新解：0341/1370（different+fence）、1340（different+rose_window）via 形状同一性传播；1386/0418（本轮 0418 因争抢未进榜，串行复测 SOLVED）via compass bbox；0209/0703 via dual_connectivity；0956（aog 35s）/1131（edge_csp 65s）为临界题受益。损失 0685（串行复测 SOLVED，争抢噪声）、0491（watchtower 8×10，**1120 时代二进制同样 OOM**，非本轮回归）。二进制 `results/bin/rsolver-65d2336-linux-x86_64`。 |
| 2026-09-18 | **最终基准（`c58054d`，回退 S5d 后）** | `results/bench/20260918_c58054d_shape-identity-compass-dual.{txt,jsonl}` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1131 / 1258** | **+11**（vs 1120） | 12 新解 / 1 损失。新解：0341/1370/1340（形状同一性）、0209/0703（dual_connectivity）、1386/0418（compass bbox）、0956/1131/1140fix/0990/1146（临界题受益于剪枝收紧）。损失仅 0491（watchtower 8×10，1120 时代二进制同样 OOM）。二进制 `results/bin/rsolver-c58054d-linux-x86_64`。 |
| 2026-09-18 | rose_growth 连通性守卫（doc 28） | 直跑 rsolver + 153 题 rose PASS 回归 | 直跑 rsolver + 全量基准 `7b1e8c5` | **1128 / 1258**（全量，噪声带内） | **0**（正确性修复，非 PASS 增益） | `try_swap_fix` / `try_chain_move` / `repair_symbol_distribution` 三处搬格点加 `is_connected_set` 守卫。**更正**：此前误判的「25 道 FAIL / 13 道新解」不成立——那 25 道题多为 PASS（rose 非法候选只是中间被拒尝试，最终由 edge_csp/aog 解出）。修复后 PASS 数不变；价值在正确性。153 道 rose PASS 0 回归。 |
| 2026-09-18 | **本轮收口基准（`8221be7`）** | `results/bench/20260918_8221be7_final.{txt,jsonl}` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1131 / 1258** | **+11**（vs 1120） | 12 新解 / 1 损失。新解：0341/1370/1340（形状同一性传播）、0209/0703（dual_connectivity D1/D2）、1386/0418（compass bbox 面积界）、1433（rose_window 单独可进门控）、0745（pieces）、0956/1131/1140fix（临界题受益）。损失仅 0491（watchtower 8×10，1120 时代二进制同样 OOM）。二进制 `results/bin/rsolver-8221be7-linux-x86_64`。 |
| 2026-09-18 | watchtower Pass B 对角格误判修复（`3262e5d`） | `results/bench/20260918_3262e5d_watchtower-fix.{txt,jsonl}` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1130 / 1258**（噪声带内） | **+1 真增益**（0496） | Pass B 原先对「顶点四格间无共享边且 value>1」一律判矛盾；blocked 格可让顶点只剩对角相邻两格（0496 的 vertex(5,2)），它们仍可经外部路径合并，value=2 可满足。改为仅角点单格且 value>1 才矛盾。**0496（7×7 watchtower）via edge_csp 7ms 解出**；0491 在不 OOM 时亦 SOLVED（本轮并行下仍被 OOM 击杀）。0209/0418 本轮为争抢/OOM 翻转（串行复测均 SOLVED）。二进制 `results/bin/rsolver-3262e5d-linux-x86_64`。 |
| 2026-09-20 | edge_csp inequality 弧一致 min/max 收窄 | `results/tmp/20260920_ineq_arc_A.jsonl` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1156 / 1258** | **+3**（vs 1153，零回归） | `propagate_inequality_clues` 增加组件级 `[min,max]` 弧一致收窄（移植参考 `pieces.rs:337-377` 的技术，edge_csp 此前只有矛盾检查）：`max[small] ≤ max[large]-1`、`min[large] ≥ min[small]+1` 不动点迭代；收窄到 `min==max` 时钉为 target 解锁封口路径。**probing 期跳过收窄**——每次 probe 跑一遍不动点是纯开销，曾把 1131 从 2.5s 拖过 40s deadline（教训：不动点循环必须在 while 开头 `changed=false`，否则死循环挂住整个子进程）。**新解 0679（ring+inequality+difference）/1207b（inequality 6×7）/1386 via edge_csp**；0152/1407a 等纯 inequality 大搜索题仍超时（收窄不足以引导搜索）。`pytest` 301、`cargo test` 28 通过。 |
| 2026-09-20 | edge_csp 过度剪枝二连修：boxy 单洞 + size_sep 合并（doc 29 §5） | `results/tmp/20260920_boxy_sizesep.jsonl` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1153 / 1258** | **+5**（vs 1148） | 两处「当前值当最终值」的声音性缺陷（参考 aog 同款，忠实移植带入）：① `propagate_boxy_nonboxy` non_boxy 单洞强制 `max_possible >= bbox_size` → 收紧为 `==`（组件可穿洞外扩成非矩形，0497 官方 region 0 即是）；② `size_separation_merge_cuts` 无条件 `contains(merged_sz)` → 仅 `merged_sz == min(max1,max2)` 或任一侧带 target 才 Cut（merged_sz 只是最小合并尺寸，0926 官方 region 14 即是）。**新解 9 道 via edge_csp**：0497/0688/0824/0921/0926/0993/1003/1091（fast-exhausted 簇 8/10）+1140fix；0171/0932 由 0ms exhausted 转正常 timeout。0685/0826/1137/1386 本轮争抢翻负（单跑 SOLVED）。诊断方法：官方 Cut 播种 + debug 构建 `Backtrace::force_capture()` 直接点名传播器。`pytest` 301、`cargo test` 28 通过。 |
| 2026-09-20 | edge_csp 门控放开 gemini（homogeneous/heterogeneous 单独可进） | `results/tmp/20260920_geminigate.jsonl` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1148 / 1258** | **+2**（vs 1146） | `is_edge_csp_capable` 的形状同一性门控从 `same/different/mixed` 扩到含 `homogeneous/heterogeneous`（gemini 等面积封口 + delta-gemini 交互本就是真实传播）。**新解 0848/0850（non_block+gemini，edge_csp 10-13s）、0826/1137 噪声带翻正**；0418/1140fix 本轮争抢翻负（单跑 SOLVED）。新进门控的 55 道 PASS 题均由 aog 先解出，12 抽样 0 回归。`cargo test` 28 通过。 |
| 2026-09-20 | **bricky_loopy 赌博剪枝修复 + OOM 兜底重试（doc 29）** | `results/bench/20260920_61df73a_bricksound-oomretry.{txt,jsonl}` + `results/tmp/20260920_bricksound.jsonl`（无兜底对照） | `benchmark_rust_solver.py --timeout 40 -j 6` | **1146 / 1258** | **+16**（vs 1130） | 17 新解 / 1 损失（0745 为并行争抢噪声，串行复测 SOLVED）。**主修复**：`propagate_bricky_loopy` 的 ring+brick 与 bricky-only 分支把「至少 n 条 Uncut」实现成「强制前 n 条 Uncut」（赌博），1378 根层误强 (2,2) 的 W/E → palisade 误判矛盾 → 官方解被剪；改为仅 `cut_count == 2`（或 bricky `== 3`）全选 Uncut。**新解 9 道 ring+brick 簇 via edge_csp**：1373/1374b/1375/1378/0834/0631/1110/0977/0978；另 0209/0418/0491/0630/0952/0969/1294/1301 噪声带翻正。**OOM 兜底**：`RustSolver` 在 exit -9 时带 `AOG_SHAPE_CAP=200000` 重试一次（默认路径 cap 仍为 0——0710 等题合法库超 10M 条目，全局 cap 必回归），1373/1375/0834/0977/0978/0969 等 6 道 aog OOM 题由此得救。`pytest` 301、`cargo test` 34、complexity gate 全过。二进制 `results/bin/rsolver-61df73a-linux-x86_64`。详见 `docs/优化/29-bricky-loopy赌博剪枝修复.md`。 |
| 2026-09-20 | edge_csp 陈旧组件快照修复 + solitary 区域数锁定 + D0（doc 30） | `results/bench/20260920_stale-comp.{txt,jsonl}` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1157 / 1258** | **+1**（vs 1156） | 三处正确性修复 + 一处能力扩展（`docs/优化/30`）。**主修复**：`propagate_area_constraints` 的子传播器串行写边后不重建组件，下一个传播器拿陈旧 `curr_comp_id`/`comp_cells`/`growth_edges` 做推理——1017 官方解 Cut 播种下 `propagate_solitary` S3 把已并入有线索区域的组件误判成「封闭且无线索」根层矛盾；顶层 `propagate_dual_connectivity` 也用陈旧 `num_comp` 把假 `cc`（10，真值 4）当成 `cc > pieces`。修法：任何子传播器报 progress 就 `build_components()`；`area_bounds` 有进展时推迟 dual 到下一轮；`build_components_growth_edges` 写边上报新字段 `build_progress`（此前写边完全不上报，不动点可能提前收敛）。另修 `get_compass_area_bounds` 的 `exact_area` 把 `-1` 当 0，以及 `compass_enum` 门槛改用 bbox 内可填充格数（半平面和系统性高估，1017 上 36 vs bbox 8–16，`MAX_AREA_THRESHOLD=12` 因此把 4 个线索全跳过）。**能力扩展**：`solitary` → `structural_pieces` 第三来源（区域数==线索格数，87/87 官方解验证，`docs/rules-guide.md` §3.14「区域数锁定」），并新增 **D0**（组件只增不减 ⇒ 最终区域数 ≤ `num_comp`；`num_comp == K` 时分区冻结、跨组件 Unknown 边全 Cut）。**新解 0629（compass+differentiation+ring，aog 超时后 edge_csp 2.3s 解出）、0685 via aog**；1140fix 在 -j 6 下争抢翻负，**串行复测 61s SOLVED**（噪声非回归）。`--rules solitary` 71→72/87。新增诊断开关 `SKIP_AOG=1`、`EDGE_CSP_SKIP=bricky,compass,compass_in_comp,compass_enum,solitary,dual,probe,area,areatgt,areachk`。`pytest`、`cargo test` 36、complexity gate 全过。 |
| 2026-09-20 | edge_csp S5d 潜在连通性 + solitary 可行集搜索序（doc 30 §6） | `results/bench/20260920_s5d.{txt,jsonl}` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1159 / 1258** | **+2**（vs 1157） | 承接上一条：D0/D2 的收益要等搜索把组件数压到 K，两件事补上这段路。**S5d**（`solitary_potential_connectivity`）：对非 Cut 边 flood-fill 得「仍可能连通」组件，可行集单点 `{i}` 的格子必须与线索 `i` 同属一个潜在组件，否则矛盾——S5a/S5b/S5c 都不看可达性；整盘一次 flood-fill，O(格+边)。**搜索序**：`select_edge` 在 `solitary_feasible_active` 时按端点可行集交集大小加分（交集越小越优先切），让组件图 `cc` 尽快爬到 K，D2 的 `exact == cc → 全 Uncut` 和 D0 的冻结规则替搜索收尾。**新解 1017（罗盘簇最小题，本条诊断线的起点）、1060 via edge_csp**；1140fix 上一轮并行争抢翻负，本轮并行也 PASS；0685 本轮并行翻负但**串行复测两次均 20s SOLVED via aog**（噪声非回归）。`--rules solitary` 72→74/87。`pytest`、`cargo test` 36、complexity gate 全过。 |
| 2026-09-20 | edge_csp 热路径 scratch 提升（id_map / pot 复用） | `results/bench/20260920_0d326d2_scratch.{txt,jsonl}` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1160 / 1258** | **+1**（vs 1159） | 卫生项：`build_components` 的 `id_map` 与 `solitary_potential_connectivity` 的 `pot` 原本每次调用都 `vec![usize::MAX; n]`，前者现在每个不动点轮次要跑多次（每个有进展的子传播器一次），后者在罗盘+solitary 题上每节点都跑，提成 `PropagationState::{id_map_buf, pot_buf}` 复用。**教训**：复用 scratch 时清理必须放函数**开头**——这两个函数都有早退 `return Err(())`，放结尾会在早退时被跳过留下脏 buffer，`id_map` 脏了 `num_comp` 直接算错，1017 从 5s 解出变成 7ms 假穷尽。**0 回归**；0685 由上一轮并行争抢翻负恢复为 PASS。`pytest`、`cargo test`、complexity gate 全过。 |
| 2026-09-21 | rose `region_match` M1 早停健全性 + `m==2` 补集封面（doc 31） | `results/bench/20260921_2785b59_rose-m1.{txt,jsonl}` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1162 / 1258** | **+2**（vs 1160） | rose 对 14 道**有官方解**的题报告 exhausted（与 doc 29/30 同签名）。根因是 `generate_all_candidates` 的 **M1 早停**：集齐符号类型就停止扩张，但该论证只在「最小完整集已落在精确覆盖的可用尺寸窗口内」时成立。1333（7×7、5 区域、`range max 10`）唯一能凑出 49 的尺寸多重集是 `{9,10,10,10,10}`，而最小 P1+P2 张成集只有 5–8 格，5 个 area combo 全枚举出来却没有 size 9/10 的候选，319ms 报假无解。修法：按 `(total, m, min_sz, max_sz)` 推出可用尺寸窗口 `[useful_min, useful_max]`，窗口宽 ≤ `WINDOW_GROW_LIMIT`(8) 时候选 BFS 一路长到 `useful_max`，否则保留原早停（0833 那类无尺寸约束的题窗口 `[1,49]`，长满会爆）。另加 **`m==2` 补集封面** `try_complement_cover`：两区域平分棋盘，seed 0 的完整候选的补集即第二区域唯一形状，逐个连通性 BFS，最坏毫秒级。**新解 1333（via rose）/ 0990（via edge_csp）/ 0745（via pieces）**；1140fix 并行争抢翻负、**串行复测 62.7s SOLVED**（噪声）。0833 仍 via rose 解出且 rose 自身 7.96s→3.8s。未救回 0974 等 m==2 大盘（候选 BFS 在集齐异类符号前就撞 `VISITED_CAP`，属范式问题，见 doc 20 P2）。`pytest`、`cargo test`、complexity gate 全过。 |
| 2026-09-21 | rose `m==2` 补集封面必须过验证器 + 尺寸预筛 | `results/bench/20260921_416c963_m2v.{txt,jsonl}` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1165 / 1258** | **+3**（vs 1162，见备注） | PR#78 的 `try_complement_cover` 只检查补集**连通性**就连着返回第一个，环顶点度 / inequality / watchtower 一概没看——1137（`ring+inequality+watchtower+rose`）上 `m==2 complement cover found` → 54ms `validation_failed` → 整条 `region_match` 路径被放弃。改成每个补集先过题目自身的 `[min_sz,max_sz]` 尺寸预筛、再过完整验证器，**只返回第一个通过的**；都不通过则 `None` 让调用方继续正常搜索。`--rules rose_window` 子集（187 道）与 PR#78 完全持平（155/32，零增零损）——这条是纯粹的正确性修复，在「第一个连通补集不合法、后面某个合法」的题上才兑现，当前语料没有这样的题。**基准 +3（0418/0826/1140fix）均为历史争抢噪声翻正**：三道题串行复测分别 67.4s / 14.2s / 61.1s SOLVED，非本次代码带来的能力提升；`-j 6` 口径的噪声带约 ±3。**0 回归**。 |
| 2026-09-21 | `shape_pattern` 独立预钉（非 rose 的 puzzle_piece 前置，doc 32） | `results/bench/20260921_56b5d56_pp-pin.{txt,jsonl}` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1162 / 1258**（并行）/ **1165**（串行复核） | **+3**（串行口径） | 0976/0606/1215 的结构是「N 个 shape_pattern 区域 + 1 个无约束大区域」，原路由下**没有任何求解器能尝试**：aog 形状库 OOM、rose 不适用（无 `rose_window`）、edge_csp 故意排除 `puzzle_piece`（试过放开，**0 新解**——边变量范式没有形状变量只能叶验证，40s 全烧盲搜）、pieces 的 DLX 没有「大无约束区域」概念（0976 2ms exhausted）、backtrack 默认禁用。新增 `puzzle_piece_pin::solve_puzzle_piece_standalone`：枚举每个锚点的二面体放置 → 不相交完整指派 → 剩余当一个区域 → 过完整验证器。两个陷阱：**deadline 必须锚到模块自己的 `Instant::now()`**（用全局 start 等于已过期）；**给完整 unit 预算**（1215 放置搜索 ~36s）。**串行复测 0606/0976/1215 全部 PASS via pp-pin**；并行下 1215 被挤爆未进榜。并行 −5（0418/0685/0745/0826/0990）**全部是历史争抢噪声题**，串行复测均 PASS。由此确认 **`-j 6` 噪声带至少 ±5**，1165 那次本就含 3 道噪声翻正，稳定能力线 = 1162 + 3 = **1165**。`puzzle_piece` 子集 160→163/171，0 回归。`pytest`、`cargo test`、complexity gate 全过。 |
| 2026-09-22 | same-tiling 同形铺砌预通路 + pieces/pp-pin/rose 改进（`feat/area-sum-piece-count`，doc 12） | `results/bench/20260922_e5d68a8_same-tiling-full.{txt,jsonl}`（全量） | `benchmark_rust_solver.py --timeout 40 -j 6` | **1172 / 1258** | **+10**（同口径 vs 1162；13 新解 − 3 争抢损失） | 新增 ⓪ `same-tiling` 预通路（doc 12）：等距窗口 CSP **0382/0383/0960**、小形状 DLX **0763**、shape_pattern 预钉×同形余数 **1098/1099/1100**；`area_sum` 区域数推导（去重值和=total ⟹ 区域数，**1183** via edge_csp）；pp-pin `combine_plain` 锚点覆盖重写 + 望塔剪枝（**0493**）；pieces compass 半平面计数修正（原 exclusive 计数与 validator 不符，0/103 语义错）+ frontier 完备枚举 + 玫瑰签名过滤 + block 矩形/shape_pattern 落点 + row_check 分解（**1004/0745/0223/0826**）。附带修复：rose 满单位预算（aog 超支不再饿死 rose）、probe 超时护栏（0312 树确定化 44529 nodes）、`RUST_PARTS` 4→6。**m=2 簇（1249/0987/1137/1149a/0974）仍未解**：watchtower 界改二区语义后 growth 正常搜索但撞 800k 状态上限 abort（AOG_DEBUG 实证 780k→800k；T-closure 剪枝不健全 + fence 星/must-split 缺单位传播，见第二部分）。串行临界题：1130/1215/0312/1137（solo 可解或近解，-j 6 争抢翻负）。`pytest` 301、`cargo test` 36+8（`clamps_zero_to_floor` 并行竞态 flake，单跑过）、complexity gate 全过。 |
| 2026-09-22 | m2 自由生长传播化二轮（XOR/星形 AC/望塔关系） | `results/bench/20260922_47d2e6f_m2prop-fast.{txt,jsonl}`（快速档） | `benchmark_rust_solver.py --baseline v2-full --skip-slow --timeout 40 -j 6` | **1172 口径不变**（快速档 1170/1175） | 0（NEW=0，0 真回归） | 无新解的传播化/门控迭代（见第二部分同日条目）：XOR 关系（必分边+rose 对）、fence 星按臂共识 AC（相邻星共享边互收窄）、望塔关系（val=1 全等/val=2 两格 XOR）、T-closure 健全化（修 S' 成墙误杀）、多类型 pin 修正、cap 2M、`has_constrained_compass` 单方向进门（0418 类 25 题 pieces 不再跳过）、密度门计入预切（0974 进 m2 门）。**m=2 簇 5 题仍未破**（40s deadline 打满）——cell-variable 与边界几何线索范式错配实证，正解=edge_csp 边变量宿主迁移。REGRESSION=2（0418/1140fix）均为争抢噪声（串行 28.9s/23.0s SOLVED）。外围 7 题零回归。 |
| 2026-09-22 | exact_piece_count 重开 + rose pair 分支声音性修复 + aog 单位预算 | `results/bench/20260922_c457981_pairfix-full.{txt,jsonl}`（全量） | `benchmark_rust_solver.py --timeout 40 -j 6` | **1176 / 1258** | **+4**（6 新解 − 2 噪声翻负） | `exact_piece_count` 重推导（doc 27 证伪前提已愈：3262e5d 修 watchtower Pass B 对角格后 two-piece parity 不再放大错强；1135/1392 复测 0.3s/1.3s SOLVED）——Cut 边 parity=1 seeding 让 **0987 40s 超时→7.4s 解出**。rose pair 分支四修：SAME 记录化（原强制任意 witness 路径不完备，0974 假穷尽→**13s SOLVED**）、Snapshot 回滚 diffs/sames（跨分支泄漏）、`select_rose_pair` 过滤 same_set（同对重复分支互递归→1340/1352 **栈溢出 -6** 修复）、pair 分支门控 parity1 源（1433/0655 64s→0.5s/1.5s）。aog 单位预算锚自身起点（原先 same-tiling 烧完后 aog 拿 0ms）→ **0268/0704/0941/0988 via aog**（~40s 整点）。新解 +6：0974/0987/0268/0704/0941/0988。m=2 簇剩 1137/1249/1149a。翻负 0418/0685 均历史争抢噪声（串行 28.9s/20s SOLVED）。`pytest`、`cargo test` 36+7、complexity gate 全过。 |
| 2026-09-22 | loop_closure 选择性移植 + 望塔精确度判死 | `results/bench/20260922_8e9d632_lc-wd-fast.{txt,jsonl}`（快速档 + 12 题矩阵） | `benchmark_rust_solver.py --baseline v7-full --skip-slow --timeout 40 -j 6` + 直跑 | **1177 口径**（+1 via edge_csp） | **+1**（1137） | loop_closure 只移植规则 1（**须挂 ring 门**——brick 的 T 连杆可合并环，1294 曾根层 5「环」误杀）+ 规则 4（max_loops==1 且 ring 时触边框边必 Uncut）；参考的 `@` 度数规则因语义不同（区域去重数 vs 割度，官方解 241/471 反例）弃。望塔精确度判死（4 满象限内部顶点：val=1⟹度0、val=2⟹{2,4}、max_loops==1 时精确-2；m=2 官方 634/634）——**强制分支有未明交互 bug 暂关**（KNOWN BUG 注释）。**新解 1137**（m=2 簇第三员）；快速档 REG=1（1294，规则 1 缺 ring 门误杀，已修，放宽单调不丢解）。`pytest`、`cargo test`、complexity gate 全过。 |
| 2026-09-23 | 1180 里程碑合并态全量实测（`4a81357`，PR#81 合并后复核 + 进度图补录） | `results/bench/20260923_4a81357_milestone-full.{txt,jsonl}`（全量）+ `results/bin/rsolver-4a81357-linux-x86_64` | `benchmark_rust_solver.py --timeout 40 -j 6` | **1183 / 1258** | **+7**（vs 1176 同口径；0 翻负） | CI「Benchmark & trend」在 PR#81 合并时被 runner shutdown（exit 143）打断，README 进度图停在 `c457981`@1176；本地全量实测补录（`docs/solver-history.json` 第 30 点 → 94.04%）并重绘 PNG / `/trend/` 页。对照 `c457981` FAIL 集双向 diff：**+7 全部可解释、0 真回归**——5 道为 8e9d632（loop_closure+望塔精确度判死）能力兑现（**1137/0990/1146/1147/1406**，快速档当时未全量复核），2 道噪声带回正（0418/0685）。1130 本轮 -j 6 下仍翻负（pieces:timeout 80s，串行 33s SOLVED；稳定线口径 1183+1=**1184**）。剩余 75 FAIL：compass+solitary 9 簇（**参考 C++ AoG_Solver 实测同超时**，1246/0312 60s/30s 零输出、对照 1283 秒解——参考盲区；`pieces::generate_compass_polyominoes` 无界方向格网爆炸 + 2000/200k 硬截断 → DLX 假穷尽）、OOM -9 四道（0224/1215/1260/1138）、watchtower/difference/inequality 碎簇。本轮纯文档/归档提交无代码改动（pytest/cargo 免跑，基准与二进制按规归档）。 |
| 2026-09-23 | **compass-part 联合划分搜索器**（`65c32e4`，doc 13） | `results/bench/20260923_65c32e4_cp-fast.{txt,jsonl}`（快速档）+ compass+solitary 簇直跑 | `benchmark_rust_solver.py --baseline 4a81357-full --skip-slow --timeout 40 -j 6` + 逐题直跑 | **1188 口径**（1183 + 簇直跑 5） | **+5** | 新增路由 ⓪b `compass_part.rs`（doc 13）：`{compass, solitary}` 纯净题的 k 区域联合划分搜索（solitary 锁 k=#罗盘线索；紧线索先生长、最松线索吃残余；半平面精确计数 + area 窗口联合收紧 + AC 强制钉格 + **补全算术剪枝** Σshort≤2·slots + 跨线索半平面容量剪枝）。**新解 5/9**：0312/0680/0681/1246/1259（0.1–3.3s，此前全灭：`pieces` 预枚举 2000/200k 硬截断假穷尽，参考 C++ AoG_Solver 同题 60s/30s 同超时——参考盲区）。0682/0683/1258/1260 首块松线索格网 >20M 状态（cap 实验确认），留 cell-labeling CSP 迭代。快速档 1182/1187：REG=1（1406，实验负载争抢，**串行 53.5s SOLVED** 噪声）、NEW=0（簇题全在 skip-slow 名单，收益以直跑计）。`pytest` 301、`cargo test` 41、complexity_gate 全过。同日 wdegree 强制分支 KNOWN BUG 侦查有进展（模型 1568 顶点 0 反例证健全；真凶为上游放大链，见 doc 27 追记），强制保持关闭。 |
| 2026-09-23 | **wdegree KNOWN BUG 结案**＋陈旧组件消费端重建（`ed2e4a3`，doc 27 §8）＋进度图横坐标抽稀（`c89c6b6`） | `results/bench/20260923_ed2e4a3_wdeg-fast.{txt,jsonl}`（快速档）+ 三题串行 | `benchmark_rust_solver.py --baseline 4a81357-full --skip-slow --timeout 40 -j 6` + 直跑 | **1188 口径不变**（强制解锁稳基线） | **0**（NEW=0，REG=3 均噪声） | 真凶结案（doc 27 §8）：组件缓存「写边即失效」，`rose_separation` 卡口 BFS 从陈旧 `comp_cells` 低估可达性 → 假卡口假 Uncut（1135 (6,4)-(7,4)）→ 级联假矛盾；wdegree 强制只是轮内写流量放大器。修复＝**谁读谁重建**（watchtower / rose_separation / rose_phase3 入口各 `build_components()`，第四次陈旧组件事故的定则入档 11-edge-csp 文档）。**singleton-fit 强制正式解锁**：1135 36ms / 1392 414ms / 1137 24s 全 SOLVED（1137 较禁用提速）；回归点 1294/1017/0987/1378/1110 全绿。快速档 1182/1187：REG=3（0685/1146/1406 串行 21s/68s/56s SOLVED，全争抢噪声）、NEW=0（收益待 m=2 配套）。另 `c89c6b6` 进度图横坐标标签抽稀（PNG ~8 刻度+45°、SVG ~10 刻度）。`pytest` 301、`cargo test` 41+8、complexity_gate 全过。 |
| 2026-09-23 | **compass-part v2 cell-labeling CSP**（`compass_label.rs`，doc 13 §6） | `results/bench/20260923_eb5bc38_cp-v2-cluster.txt` + `results/bin/rsolver-eb5bc38-linux-x86_64` | 直跑 `rsolver`（RSOLVER_TIMEOUT_MS=40000）+ `cargo test --release` | **1190 口径**（1183 + 簇 7） | **+2**（0682/1260） | 松窗口残簇改打**格→标签 CSP**（doc 13 §6）：势连通域（多源位集 worklist）+ 半平面基数 AC（过近似池健全强制）+ **象限联合计数 `joint_ok`**（象限格双吃两方向，精确小目标被超额满足——逐方向池看不见，对和界区间传播一步看穿；按 (标签,8分区) 计算 n×k→8×k 提速 3.1×）+ 尺寸窗 + 可桥接性。搜索：**前沿取格**（树序交错保完备）+ 邻接优先值序。向导统计法（官方路径逐步打印值序排名）实证：S1/S31 等「不可区分对称/蛇尾远端」全是 MRV 跳跃取格的人工产物，前沿序后 0682 从爆帽（4M）变 2.1M 节点解出。**生产链**：v1 短缰绳（timeout/3，8–12s）+ v2 吃剩余预算——v1 簇 5 题 12s 内零回归（1246 最慢 4.3s）。新解 **0682（17.4s）/ 1260（12.9s）**；残簇 0683（19M 节点 60s）/ 1258（k=44，1.2M/60s）deadline 截断非穷尽（审计/向导测试证传播健全），测试 `#[ignore]` 挂起跟踪。`pytest`、`cargo test` 45+8、complexity_gate 全过。 |

---

> **各里程碑 Zone 明细**（从主表「备注」拆出，按日期对齐）：

| 日期 | commit | Zone | 通过 | 未解 | 变化 |
|---|---|---|---|---|---|
| 2026-08-05 | `33d32c5` | Zone1 | 300 / 312 | 12 | — |
| 2026-08-05 | `33d32c5` | Zone2 | 387 / 438 | 51 | — |
| 2026-08-05 | `33d32c5` | Zone3 | 329 / 481 | 152 | — |
| 2026-08-06 | `7e569e7` | Zone1 | 300 / 312 | 12 | 0 |
| 2026-08-06 | `7e569e7` | Zone2 | 393 / 438 | 45 | **+6** |
| 2026-08-06 | `7e569e7` | Zone3 | 328 / 481 | 153 | -1 |
| 2026-08-06 | dfadfe3 | A/B/C | 26 / 27 | 1 | 0 |
| 2026-08-06 | dfadfe3 | Zone1 | 301 / 312 | 11 | +1 |
| 2026-08-06 | dfadfe3 | Zone2 | 395 / 438 | 43 | +2 |
| 2026-08-06 | dfadfe3 | Zone3 | 330 / 481 | 151 | +2 |
| 2026-08-07 | `6169df3` | A/B/C | 26 / 27 | 1 | 0 |
| 2026-08-07 | `6169df3` | Zone1 | 300 / 312 | 12 | -1 |
| 2026-08-07 | `6169df3` | Zone2 | 393 / 438 | 45 | -2 |
| 2026-08-07 | `6169df3` | Zone3 | 327 / 481 | 154 | -3 |
| 2026-08-07 | fence-midsearch | A/B/C | 26 / 27 | 1 | 0 |
| 2026-08-07 | fence-midsearch | Zone1 | 300 / 312 | 12 | 0 |
| 2026-08-07 | fence-midsearch | Zone2 | 394 / 438 | 44 | +1（0829） |
| 2026-08-07 | fence-midsearch | Zone3 | 327 / 481 | 154 | 0 |
| 2026-08-08 | rose-pp-pin | A/B/C | 26 / 27 | 1 | 0 |
| 2026-08-08 | rose-pp-pin | Zone1 | 301 / 312 | 11 | +1（0732，rose） |
| 2026-08-08 | rose-pp-pin | Zone2 | 397 / 438 | 41 | +3（0710/1320/1348，aog timeout 修复） |
| 2026-08-08 | rose-pp-pin | Zone3 | 328 / 481 | 153 | +1（0685，aog timeout 修复） |

---

## 第二部分：变更内容

> 按时间顺序**往后追加**（旧的在上）。每次：日期、commit、改了什么、结果。

### 2026-08-05 · 校验 / 转换修复（commits `4ab9e4b` `047f9a1` `9a6c965` `bde3713`）
早期全量扫描发现 129 道「求解器解 ≠ 官方解」，逐一深挖后确认**绝大多数并非真多解，而是转换 / 校验的真实 bug**：

1. **gemini/delta 边约束未被强制**（`4ab9e4b`）：`build_rules` 只给 inequality/difference 生成规则类型，`=`/`!` 边没有 `homogeneous`/`heterogeneous` → aog 求解器跳过边约束、`IndependentValidator` 也不分发边检查 → 求解器产出违反边约束的划分却被判合法。
2. **玫瑰窗检测读错位置**（`047f9a1`）：`_is_rose_window` 用固定 2 字符步长切原始网格找 P 符号，前面有变宽格（罗盘 `U…` / `S` 形）时位置错位 → rose_window 规则被静默丢弃（如 0634）。
3. **环纹规则漏边框 T 型**（`9a6c965`）：环纹检查只遍历内部顶点，内部区域边界与外边框相遇也是 3 段 = T 型（如 0638）；另 `check_rule_ring` 误用 `Edge.is_boundary` 而非区域边界。
4. **1SPR 的 S 格缺 shape 约束**（`bde3713`）：1SPR 谜题 `S#` 格只记 symbol 未加 `shape_pattern` → 缺 puzzle_piece 约束。

**补充**：`= (gemini)` / `! (delta)` 边语义是「两侧区域同形/异形」，非「同区域」——校验器与游戏文档（glimmith-solver）一致，非 bug。
**结果**：129 道 → 6 道（全为 watchtower，见附录 A）。

### 2026-08-05 · block / solitary 建模修复 + block→形状池（commit `e926943`，另一窗口）
方块题专项：全语料 66 道 block 题，逐题硬超时复扫。
- **根因**：Rust 回溯/pieces 把 block 候选面积硬约束为 4（`min_a=max_a=4`）、`check_block` 要求全 2×2；aog 才是正确的「任意矩形」。`solitary` 同病（误当面积=1）。
- **修复**：① block→形状池转换（`rust_solver.py` 合成"所有矩形"注入载荷，`pieces` DLX 接手）；② 修 Rust block/solitary 建模（`backtrack.rs`/`pieces.rs`/`constraints.rs`，`check_block`→`is_rectangle`、`check_same/different`→按形状等）；③ DLX 迭代到合法划分（`dlx.rs search_with_check`）；④ 预算语义改「单元预算」（每部分各拿完整 timeout）；⑤ 形状池按盘面预筛。
- **结果**（单元预算 20s）：0908v2、0826、0829 新解出（DLX）；0446、1109、1004 仍 UNSOLVED（DLX distinct 形状剪枝 / compass 专项 / rose 组合+内存泄漏，见后续计划）。
- 剩余方向：0446（形状去重剪枝）、1109（compass 剪枝）、1004（先修回溯 rose-parallel 内存泄漏）。

### 2026-08-05 · rose 求解器下沉 Rust（commit `4733f59`）
把 Python rose 求解器（`region_match.py` + `rose_growth.py`）移植进 Rust（`rsolver/src/solver/rose/`），使 Rust 二进制能解 aog 解不出的**无尺寸约束纯 rose**。设计文档：`docs/重构/rose-solver-rust-port.md`。
- 模块：`cells.rs`（CellSet 位集/边键/PreBoundaries）、`region_match.rs`（候选 BFS + 面积/分区过滤 + 面积组合 + MRV）、`rose_growth.rs`（wavefront + swap/链式修复，单/多符号）、`mod.rs`（入口，`aog::validate` 作验收门）。
- 分发：rose-capable 题 aog 先 5s 预算（保住 ~30 道 <1s 已解），失败后 rose 用剩余预算。
- **结果**：纯 rose 语料 30 题 28 题可解；C4-1/0277/0213/0213nopad 新解出且与官方一致；大网格（0804/1433/1434）仍 UNSOLVED（Python 也解不出，无回归）。

### 2026-08-06 · rose 尺寸感知优化（commit `7e569e7`）
对比 Python rose 与 Rust rose，找到并修复 **range+rose 差距**（带区域尺寸约束的玫瑰窗）：

| 题 | Python rose | 优化前 Rust | 优化后 Rust |
|---|---|---|---|
| 1334（range+rose, 7×5） | 9.4s | 30s FAIL | **438ms ✓** |
| 1342（range+rose, 6×6） | 30.9s | 30s FAIL | **865ms ✓** |

- **根因**：① `region_match` 面积组合 `min_area_per_region=1`，带尺寸约束的题组合爆炸（1342 达 **1265 万组合**，~1GB）；② 候选 BFS 20000 上限 + 位集遍历顺序与 Python frozenset 不同，截断保留的候选不一致。
- **修复**：`rose::region_size_bounds()`（解析 range/precise 全局区域尺寸界）+ region_match 按 `[min,max]` 过滤候选、组合 `min_val=max(min,N)`。1342 组合 1265 万 → **1 个**，1334 → 6 个。`AOG_ROSE_BUDGET_MS` 5s→3s（aog 解 <1s 纯 rose 后，硬题快速交 rose）。
- **结果**：Rust-only 基准 1047/1258（rose 前 1040）。0 个 rose 专属差距；剩余 2 个 Rust-only 缺口（0957 brick+block+rose、1301 brick+area）为 **brick 回溯短板**（非 rose），router Python 兜底覆盖，实际无回归。

### 2026-08-06 · brick 回溯短板修复 + 砖纹规则语义修正 + 形状规则语义修复 + aog 预算回退（本会话）
针对 1301 / 0957 两个 **brick 回溯短板** 与全量回归中发现的规则校验漏洞，一组联动修复：

1. **backtrack area 剪枝落地**（§3.1 设计）：`pick_next_cell` 动态连通优先生长线索区域 + `check_area_lower_bounds` 密封/容量剪枝 + frontier 引用计数。行优先的死结（`(0,3)` 先于 `(1,1)` 被处理导致 48 区域长不到它旁边）被绕开。
2. **砖纹（brick）规则语义修正——两处方向相反的 bug**：
   - 旧 `vertex_boundary_count` 把 **blocked 相邻全按边界计** → 棋盘角落（3 blocked + 1 区域）假报 4 路交叉，回溯提前误剪；
   - 修复中一度把「顶点有 blocked 就跳过 brick」→ **放过真 4 路交叉**（1 blocked + 3 个不同区域 = 4 路交叉），于是 1301 出现**孪生解**（单点 (7,6) 被误判合法，官方是 (6,7)）。
   - **最终正确语义**（镜像 C++ `check_tatami` / glimmith-solver）：blocked 当空区、**blocked-blocked 不算边界、blocked-区域算边界、不跳过 blocked 顶点**。同步修复 `validate.rs`、`IndependentValidator._check_brick`、backtrack。
   - **结果**：1301 唯一解 = 官方解 (6,7)。
3. **删除 `check_merge_ok`**（backtrack 过度保守剪枝）：「加入格若触及别的区域就拒绝」把 1301 官方解构造（单点 (6,7) 紧挨区域 0）整支剪掉 → 回溯找不到官方解。删除后 **1301 与 0957 均由 Rust backtrack 解出**（0957 ≈1.9s；1301 约 30s = aog 30s 预算 + backtrack 秒级）。正确性由叶子校验 + `check_all` + `IndependentValidator` 三层兜底。
4. **形状规则语义修复**：`constraints.rs` 的 `check_same`/`check_different` 用原始 `shape` Vec 比较 → 改为 `dihedral_key` 规范键；`check_mixed` 由 `!check_same`（全局近似）改为「相邻区域形状不同」正确语义（镜像 validate.rs / Python）。backtrack 叶子 `different` 检查同步修复。**结果**：修复 1114 等 `different` 题被旋转/翻转重复形状误放行的问题（`IndependentValidator` 一直能拦，Rust-only 之前会接受错解）。
5. **aog 预算回退**：全量回归发现上一版 `AOG_BUDGET_CAP_MS = 1s` 把 aog 硬性限死，**65 道 aog 在 1-25s 能解的题全部转 FAIL**（1047 → 983）。移除 1s 封顶、aog 拿回完整 `timeout_ms`；配合热路径 deadline 检查（Fix B/C：shape 循环每 256 查、size 循环每次查），aog 在 deadline 处**精确停住**而非烧光预算。

- **验证**：`cargo test` 9 通过；`pytest` 387 通过；全量 Rust-only 基准 **1052/1258**（0 真实回归，见第一部分最新条目）；router 实测 1301（≈30s）/0957（≈1.9s）均 `rust(ok)`，不再依赖 Python 兜底。

### 2026-08-06 · 评估并移除 Python 求解器（plan C.0 完成）
**评估**：Rust-only 全量基准 1052/1258 之后，Python 求解器（exact_cover / rose / backtrack，及 dlx / candidates / region_match / rose_growth / bfs_candidates / polyomino_cache / checks / propagator / validator）是否还有解出价值？
- **历史全路由扫描**（Aug 5，brick 修复前）：~1000+ 官方题中 Python 兜底只解出 **5 道**（C4-1 / 0277 = rose、1169 = exact_cover、1301 / 0153 = backtrack），逐一核对**现均由 Rust 解出**（均不在 206 道 Rust-only 失败清单）。
- **定向扫描**：对 206 道 Rust-only 失败题跑全路由（含 Python 兜底），处理 82/206（40%）**Python 兜底 0 命中**（81 次尝试全败）；唯一解出为 Rust（0745，58s，超 bench 40s 时限）。因已知 rose 内存压力（单进程 4.3GB/15GB）中止，剩余 Zone3 硬题为能力极限、Python 亦从未解出。
- **结论**：Python 求解器对官方语料无解出价值，移除。

**移除内容**：
- `default_router` 改 **Rust-only**（`RustSolver()`）。
- 删除 `src/solver/`：backtrack / dlx / candidates / bfs_candidates / region_match / rose_growth / polyomino_cache / checks / propagator / validator / exact_cover / rose；`src/services/solver_service.py`（UI 已死）；调试脚本（show_candidates / debug_222 / test_111 / test_222）。
- **保留共享层**：`constraints.py`（RULE_CHECKERS）、`shapes.py`、`exceptions.py`、`src/validation/validator.py`（IndependentValidator）——UI 编辑器（shape_editor/shape_gallery）、生成脚本与独立校验依赖。
- `gen_ai_puzzles.py` 改用 router 校验；`main_window.py` 去掉 SolverService。
- 测试：删 test_backtrack / test_propagator / test_validator；test_solver_end_to_end 改为 router 端到端（26 个）；conftest 去 solver/validator fixture；test_constraints 内联 `_sync_boundaries` 助手。

**验证**：`pytest` **290 通过**（删 Python 求解器相关 ~97 个后新基线）；`cargo test` 9 通过；Rust-only router `benchmark_rust_solver.py --dir puzzles/official/Zone1` **301/312** 与 dfadfe3 基准 Zone1 完全一致，**0 回归**（见第一部分最新条目）。

### 2026-08-06 · P0 重构：validate.rs 独立 + 收敛 5 处重复实现（本会话）

纯重构，**无行为变化**（求解数字不变，基准豁免）。
1. **validate.rs 提升为独立模块**：`solver/aog/validate.rs` → `solver/validate.rs`，
   消除 rose 依赖 aog 的反向依赖；`aog/mod.rs` 出口与 `rose/mod.rs` 验收改走
   `crate::solver::validate::validate`。
2. **新建 `shapes.rs` 收敛 5 处重复实现**：
   - `dihedral_key`（constraints / validate 两份 → `shapes.rs:32` 唯一）；
   - `is_rectangle`（两份 → `shapes.rs:12` 唯一）；
   - `collect_pool_shapes`（aog/core 与 constraints 的双来源收集 → `shapes.rs:75` 唯一）；
   - `area_bounds`（pieces/backtrack/rose 三版合并 → `shapes.rs:115`；统一默认 max=h*w、
     罗盘派生 min；rose 侧因 `region_match` 再 `min(total-(m-1))` 重界，行为不变）；
   - `rose_symbol_types`（rose/aog/validate 三处内联 → `shapes.rs:160` 唯一；空
     `symbol_types` 数组回退格子符号的语义统一，语料无空数组题，边界不触发）。
3. `check_mixed` 已统一的「相邻异形」语义**保持不变**（本轮只收敛，不动实现）。

- **验证**：`cargo test` 9 通过；`pytest` 290 通过；Zone1 `benchmark_rust_solver.py --timeout 25 -j 8`
  **301/312**（11 失败与 rustonly-router 基准组成一致，**0 回归**）。
- 注：0213 / 0213nopad（大 rose）在 -j8 负载下偶发互换超时（本轮 0213 超时、上轮 0213nopad
  超时），单跑各 ~2.5s 解出——与已知 0833 / 0882 同类负载波动，非回归。

### 2026-08-06 · P1 性能/内存：backtrack 扁平数组 + Pools 惰性分配（本会话）

纯性能/内存优化，**求解行为不变**（同 DFS 逻辑）。
1. **backtrack 状态扁平数组**（#3）：`cell_to_region` `HashMap<(usize,usize),usize>` →
   `Vec<Option<usize>>`（`r*width+c` 索引）；`region_shapes` `HashMap<usize,Vec>` →
   `Vec<Vec<[usize;2]>>`（区域号严格 0..n 递增、回退递减，`push`/`pop` 维护）。`frontier` /
   `region_clue` 保持 HashMap（area 门控）。`BacktrackState` 加 `width` stride 字段。
2. **边界尊重检查扁平数组**（#4）：`regions_respect_boundaries`（`mod.rs:125`）的
   `HashMap<(usize,usize),usize>` → `Vec<Option<usize>>` 直接索引。
3. **aog Pools 惰性分配**（#5）：`Pools.place` `Vec<RefCell<PlaceLevel>>` →
   `Vec<RefCell<Option<PlaceLevel>>>`，`Pools::place_level(i)`（`RefMut::map` +
   `get_or_insert_with`）按 DFS 深度惰性建层。**峰值 RSS 实测**（`results/tmp/20260806_pools-lazy-rss.txt`）：
   A1-1 **5.6→2.3MB**、C1-3 **5.7→2.9MB**、C4-1 **11.7→9.0MB**（此前 100 层 × ~33KB 常驻 ~3.3MB）。

- **验证**：`cargo test` 9 通过；`pytest` 290 通过；Zone1 `benchmark_rust_solver.py --timeout 25 -j 8`
  **301/312**（与基准一致，**0 回归**）。定向：1301（brick+area，backtrack ≈30s）、0957
  （brick+block+rose ≈1.8s）、C4-1（rose ≈3.1s）、A1-1（shape_pool 3ms）均正常解出。

### 2026-08-06 · P2/P3 清理：死代码移除 + Cell 求解状态分离（本会话）

纯清理，**无行为变化**（编译期确认移除项均 0 调用）。
1. **死代码移除**（#6）：`apply_line_constraint` 的 `vertical` 参数（调用点恒传
   `cell1_first=true`）；`grid::unassigned_cells` / `connected_components`、
   `polyomino::generate_polyominoes`、`aog::core::dbg_steps`、`Dlx::search` +
   `solution_rows` + `header_count`、`CellSet::set_from`、`PreBoundaries::len`；
   `types::Direction` 枚举与 `CompassClue::get`；`pick_next_cell` 未用的 `puzzle`
   参数、`check_edge_constraints` 未用的 `regions` 参数、`has_shape_pool` 重复声明、
   `slash_check_enable`/`slash_check_slash_cnt`（只写不读）。`Solution.steps_taken`
   **保留**（JSON 兼容）并标注废弃。`main.rs` 文档字符串修正（`--parse` 未实现）。
2. **Cell 求解状态分离**（#8）：删除 `Cell.region_id`（求解路径死字段，16B/格）与
   `assigned()`；依赖它的 `unassigned_cells` 已随 #6 移除。Cell ~192B → ~176B。
   Python `board.py` 的 `Cell.region_id` 是独立模型（Board 重建用），不受影响。

- **验证**：`cargo test` 9 通过；`pytest` 290 通过；Zone1 `benchmark_rust_solver.py --timeout 25 -j 8`
  300/312（11 个基线失败 + 0213/0213nopad 这对大 rose 同轮双双超时——负载波动，单跑各
  ~2.5s 解出，**非回归**）。构建警告从 ~20 降到 2（`is_subset` 测试辅助、`L` C++ 镜像命名）。

### 2026-08-06 · P2 #7：批量模式（子进程复用）+ IO 移出 main.rs（本会话）

解决 `benchmark_rust_solver.py` / `benchmark_rust_solver.py` 每题 spawn 一次 rsolver 的启动开销：
- **rsolver `--batch`**：从 stdin **逐行读**多份紧凑谜题 JSON，逐题求解、**逐行输出**
  题解 JSON（1 输入行 ↔ 1 输出行；坏行输出 `solved:false` 继续）。单题模式（文件/单段
  JSON）完全不变。
- **IO 移出 main.rs**（用户要求）：新建 `src/io.rs` 承载 JSON 模型 / `build_puzzle` /
  序列化 / `solve_json_line`；`main.rs` 只做 stdin/argv/stdout 调度。
- **`RustSolver.solve_batch`**：一个 `--batch` 子进程批量求解，**每题独立预算**
  （`select` + `os.read` 逐行读，超时只截断该题与后续题，已完成的保留）——与单题模式
  每题的墙钟上限一致，大 rose runaway（如 C4-2）不会烧掉整批预算。
- **`benchmark_rust_solver.py --batch N` / `benchmark_rust_solver.py --batch N`**：文件分块，
  每块复用一个子进程（默认 1 = 逐题，行为不变）。

- **验证**：`cargo test` 9 通过；`pytest` 290 通过；C 区 batch 与单题均 **4/5**（C4-1
  保留解出、C4-2 预算内截断）。`ruff check src/` 无新增（`supports` ARG003 为历史问题）。
- **已知局限**：批量逐进程顺序求解，某题若超出内部 30s 预算（已知大 rose runaway，
  如 C4-2 / 0804 / 1433，见附录 B），**同批排在其后的题会连带判超时**（每题仍独立
  截断、已完成者保留）。reference 集 batch 8 5/22 vs 单题 15/22 即此连带所致。
  **精确验证请用默认 `--batch 1`**；`--batch N` 适用于良性集合的吞吐扫描（快题实测
  提速 ~5×，spawn 开销 ~1ms/题 → ~1.3s/1258 题）。批量模式交付子进程复用架构
  （`--batch` 协议 + `io.rs`），不改变任何求解结果。

### 2026-08-06 · 删除 constraints.rs，build_solution 改用 validate.rs 全量复查
- **背景**：2026-08-06 全量 verify 暴露 **30 题「答案未通过独立验证」**，全部涉及
  fence / compass / ring / rose_window 等规则。根因：`constraints.rs` 的 9 条规则是
  恒 `true` 的 stub——aog 对 ring+fence+rose 等组合题预算内解不出时，backtrack/pieces
  产出的错误解通过 stub 被标 solved，只被 Python Router 的 IndependentValidator 拦下
  → 判 FAIL。30/30 官方解通过 Python 验证器（非谜题/转换/规则理解问题，纯求解器代码问题）。
- **改动**：
  1. 删除 `rsolver/src/constraints.rs`（含 4 个单元测试；`is_rectangle` 测试移入 `shapes.rs`）。
  2. `solver/mod.rs` 的 `build_solution` 与 `solver/pieces.rs` 改用
     `crate::solver::validate::validate`（与 aog 出口 / rose 验收同一闸门，覆盖全 22 规则）
     做全量复查。
  3. 消除 `check_heterogeneous`/`check_homogeneous` 的语义分歧：旧 `constraints.rs` 是
     区域级全局检查，validate.rs / Python 是**边级**（只查带 `==`/`!` 标记的边）——现在
     唯一语义是 validate.rs 的边级。
- **验证**：`cargo test` 6 通过（4 constraints 测试删 + 1 shapes `is_rectangle` 加）；
  `pytest` 290 通过；40 抽样 PASS 题 + 10 ring/compass PASS 题 **0 回归**；
  36 抽样解出的官方题与 `*-answer` 官方解一致（0 DIFF）。
- **benchmark 脚本官方解验证**：`benchmark_rust_solver.py` / `benchmark_rust_solver.py` 对每个
  解出的官方题比对 `*-answer` 官方解分区——新增 `src/validation/official_answer.py`
  （`matches_official_answer`），结果字段 `matches_official`（True/False/None），
  False（解合法但 ≠ 官方唯一解）标记 **DIFF** 并计入失败。

### 2026-08-06 · 边界望塔缺失修复（顶点绝对坐标约定）
- **背景**：用户报告 0800/0543 官方题与 JSON 有差异——官方题在**外边界顶点**上也有
  望塔限制，JSON 缺失。经调研：14 个 watchtower 谜题有边界望塔，**6 个 watchtower
  DIFF 题（0543/0544/0662/0663/0800/1144）全在其中**。此前被误判为「多解/规则理解」。
- **根因（转换 + 模型双层 bug）**：
  1. `scripts/convert_archive.py` 只在 `1 ≤ r ≤ height-1`、`1 ≤ c ≤ width-1` 收集望塔，
     丢弃上下左右四条边界的望塔。
  2. Rust `io.rs` 顶点数组是 `(h-1)×(w-1)`（仅内部顶点），`build_puzzle` 对边界坐标
     `return Err("vertex out of range")`——模型根本不能表示边界顶点。
- **修复**：
  1. **顶点约定改为绝对网格坐标** `(0..=h × 0..=w)`：`rsolver/src/io.rs`（数组 `(h+1)×(w+1)`、
     接受边界坐标）、`solver/validate.rs` / `backtrack.rs` / `pieces.rs`（watchtower 统计
     在界非阻塞周围格）、`solver/aog/core.rs`（雷达编码 `(2r+2, 2c+2)`，原 `2r+4`）、
     `src/models/board.py`（`_build_vertices` / `cells_surrounding_vertex` /
     `edges_surrounding_vertex`）、`src/validation/validator.py`、`src/ui/grid_widget.py`
     （绘制与点击映射去掉 `±1` 偏移）。
  2. **转换器** `scripts/convert_archive.py`：收集 `0..=height × 0..=width` 全部望塔，
     绝对坐标 `(r,c)`。
  3. **迁移 85 个 watchtower 谜题 JSON**：以 `third_party/archiveofglimmith.github.io/
     puzzles.json` 为权威源（游戏解析约定：顶点行角点 `3c`，行补齐 `3W+2`），覆写
     `vertices` 字段（内部顶点重索引 + 边界新增）。
- **验证**：watchtower 专项 verify **50 PASS / 35 FAIL / 0 DIFF**（35 FAIL 全部为基线既有
  失败，**0 回归**）；6 道 DIFF 题经 router 解出且与官方解一致；0985（原 DIFF）加约束后
  30s 超时（不再出错误解）；官方解对 0985 完整约束通过。`cargo test` 6 通过、`pytest` 全绿。

### 2026-08-06 · 题解 JSON 新增 `solver` 字段（结果归因）
- **改动**：`Solution` 增加 `solver: String`，标出答案出自哪个 Rust 模块
  （`aog` / `rose` / `pieces` / `backtrack`；错误、空盘、超时占位解为空串）。
  `rsolver`（`types.rs` / `solver/mod.rs` / `io.rs`）与 Python
  （`src/models/solution.py` / `src/solver/rust_solver.py`）同步透传；
  `benchmark_rust_solver.py` / `benchmark_rust_solver.py` 以 `via=...` 输出归因。
- **意义**：把每个结果归到具体求解器，便于定位「哪个模块对哪些题有短板」——
  例如 0401/0437/0439/0459/1011a/0586 的「fails rule validation」现可确认为
  **backtrack 模块**的解被 `validate.rs` 拒绝（而非 aog）。
- **不改变求解能力**：纯协议/归因改动，官方基准数字不变（1052/1258）。

### 2026-08-07 · 搜索优化 4 项：约束边→边界 + 中搜索形状剪枝 + BF 默认开启 + 拓扑预检（commit `6169df3`）

针对 134 道 FAIL 题的规则画像（`docs/优化/10-专用求解器方案.md`），实施 4 项低风险增量优化：

1. **约束边强制为边界**（`rsolver/src/io.rs`）：
   inequality / difference / heterogeneous / homogeneous 边在解析时设为
   `is_boundary = true`。之前只有 aog 内部编码（`core.rs` LINE_BLOCK），
   现在 backtrack / pieces / rose 通过 `is_adjacent_free()` / `is_precut()` 自动受益。

2. **密封区域形状规则即时检查**（`rsolver/src/solver/backtrack.rs`）：
   新增 `check_sealed_regions()`，每次 `frontier_assign` 后调用。区域密封时
   立即检查 different / same / block / non_block 约束，不等叶子。
   无状态设计（每次从 `region_shapes` + `frontier` 重新计算），无需 undo 逻辑。

3. **Bellman-Ford 面积传播默认开启**（`rsolver/src/solver/prototypes.rs`）：
   gate 从 `BF_PROPAGATE=1`（opt-in）改为 `BF_PROPAGATE=0`（opt-out），
   每 256 步传播 inequality / difference 边的面积约束。

4. **搜索前拓扑校验**（`rsolver/src/solver/mod.rs`）：
   新增 `pre_search_topology_check()`，O(V) 扫描所有顶点，检查预画边界 + 约束边
   是否已违反 ring / brick。正确阈值：Ring 只在 `def_boundary==3 && unknown==0`
   时拒绝；Brick 在 `def_boundary>=4` 时拒绝。使用单元格几何
   `(vr-1,vc-1),(vr-1,vc),(vr,vc-1),(vr,vc)` 判断每条边的状态，区分网格外（计为
   非边界）和外边界（计为边界），避免角落顶点过度计数。

**bug 迭代**：拓扑预检经过 3 轮修复——数组越界（`h_edges` 维度 `[h][w-1]` 误用 `vc < w`）、
外边界过度计数（角落"两面都不邻接网格格"的边不计为边界）、Ring 阈值过于激进
（`def_boundary==3 && unknown>0` 可能变 4，ring 允许）。

**结果**（20s 超时 vs 基线 40s）：0 回归、8 道新解出、0 panic、0 拓扑误判。
详见第一部分最新条目。

### 2026-08-08 · rose 解除 puzzle_piece 禁令 + 预钉 shape_pattern 区域 + timeout 透传修复（分支 `rose-pp-pin`，commit `bd2f5f5`）

**背景**：puzzle_piece 规则覆盖 171 道官方题，158 PASS（全 via aog）/ 13 FAIL。数据证实 backtrack
在 puzzle_piece 题 **0 次触发**（aog 原生支持 `AREA_SHAPE_INDEX_BIT`），故"改 backtrack 利用拼块
约束"方向无意义。13 FAIL 聚类后，4 道 `puzzle_piece + rose_window`（0732/1098/1099/1100）是最明确
靶点：aog 因 rose-capable 只拿 3s 解不出，rose solver 又在 `region_match.rs:285-291` 硬拒
puzzle_piece 题。

**改动**：
1. 新增 `rsolver/src/solver/rose/puzzle_piece_pin.rs`：
   - `dihedral_variants`：pattern 的 ≤8 个 dihedral 变体去重。
   - `placements_for_variant`：枚举使锚点落在变体内的合法放置（全在网格、不压 blocked、不跨预画边界）。
   - `enumerate_pin_candidates`：符号约束过滤（per-type 计数相等）。
   - `enumerate_pin_assignments`：多锚点笛卡尔积（互不重叠 + 余数平衡）。
2. `rose/mod.rs::solve_rose` 加 `solve_rose_with_pin` 分支（门控 `ROSE_PP_PIN`，默认开）：
   预钉 → 缩减 all_positions + 算 m' → region_match → `merge_pinned` 合并 → `accept_if_valid`。
   m'=1 快速路径 `try_single_region`（剩余格单 4-连通分量直接成区域，避开 region_match
   `CANDIDATE_CAP=20000` 候选截断）。
3. 解除 `region_match.rs:285-291` 的 puzzle_piece/shape_pool 硬禁令。
4. 修复 region_match 种子收集：`seeds` / `all_seed_cells` 改为只从 `all_positions` 收集
  （原从全盘 `puzzle.cells`），使预钉移除符号格后 `seeds.len() == m'` 自动成立。
5. **配套 timeout 透传修复**：`main.rs::resolve_timeout_ms` 读 `RSOLVER_TIMEOUT_MS` env var
  （默认 30 000，下限 1 000，含 8 个单元测试）；`io::solve_json_line(line, timeout_ms)` 接收参数
  （原硬编码 30s，`--timeout 40` 到不了 Rust 搜索）；`RustSolver._subprocess_env` 从 `--timeout` 设入、
  `_wall_budget` 给 `3×timeout×1.2` slack 墙钟；移除 `solver::solve` 里 rose 阶段的 `ROSE_TIMEOUT_MS=30s`
  clamp。详见 `docs/rust-solver/01-总体架构.md` §1/§3.2。

**关键发现**：shape_pattern 是 **dihedral 形状类**（`validate.rs:181-191` 比对 `dihedral_key(&region.cells)`
vs `dihedral_key(pat)`），不固定具体边——预钉需枚举 dihedral 变体放置。0732：2 变体 × 7 放置 = 14 候选，
符号约束过滤后唯一 1 个 = 官方解（9 格十字含 P1/P2/P3 各 1）。

**验证**：`cargo test` 16 通过（+2 puzzle_piece_pin + 6 timeout 测试）；`pytest` 全过；0732 单题 SKIP_AOG
下由 rose 解出（2 区域），正常路径下 aog 3s 失败后 rose 兜底解出。puzzle_piece 子集基准：official
159/171（基线 158，+1 = 0732，0 回归）。

**结果**（全量 `benchmark_rust_solver.py --timeout 40 -j 8`，与 `cd40cab` 基线逐题对比）：
**1052 / 1258**（+5），**0 回归**（0 PASS→FAIL）。新解出 5 道——**0732**（via rose 3002ms，rose-pp-pin
直接收益）；0685/0710/1320/1348（aog 临界题，timeout 修复后拿满 40s 预算解出）。FAIL 模式 116 无解 +
69 超时 + 21 OOM，**0 校验失败**。OOM 11→21 为 rose/aog 拿满预算后内存压力增大（原本 FAIL 的题，
非回归）。详见第一部分最新条目。

**同期证伪**：fence 预推导 DSU 方向（`fence-anchor-bfs` 分支）基于"fence_pattern arm 位 = 具体边
Boundary"的错误假设，0390 上误判矛盾，未合 main。fence_pattern 是 dihedral 形状类不固定边，DSU 预
合并不成立。详见 `docs/优化/10-专用求解器方案.md` §3.3 警示框。

### 2026-08-14 · edge_csp 边变量 CSP 独立求解器第一迭代（`edge-csp-solver`）

按 `docs/优化/14-边变量CSP独立求解器方案.md` 落地第一迭代（`docs/rust-solver/11-edge-csp求解器.md`）：

1. **新增 `solver/edge_csp/` 模块**（types/grid/adapter/prop/mod）：边变量 CSP（`Vec<EdgeState>` 三态边
   内部维护，**不动全局 `Edge` 的 52 处读取**）。从 `third_party/aog` 1:1 移植传播引擎（顶点度
   `bricky_loopy`、`build_components` 面积枢纽、`propagate_area_bounds`/`inequality`/`diff`、探测
   `probe_one_round`/`probe_pair_round`）+ 边 DFS（`select_edge` 多因子评分 + `backtrack_edges`），
   剥掉 tracing/rose/shape/match 无关部分。入口 `solve_edge_csp` 输出**先过 `validate::validate`
   复查才返回**（只 false-negative，不 false-positive）。
2. **关键正确性修复（与参考实现的差异）**：参考 aog 的 `bricky_loopy` 只数内部边、且不在叶节点验证
   ring/brick；本项目 `validate::count_boundary_edges_at_vertex` 把**外边框与 blocked 格边**也算边界。
   本移植改 `propagate_bricky_loopy` 按后者语义数度（fillable-非fillable=边界、非fillable-非fillable
   ≠边界），否则 ring 题会先找到"9 单格"等带边界 T 型的错误解被验证器拒（0666）。同时补 ring+brick
   组合（度≤2）分支（参考 `else if` 只跑 loopy）。
3. **路由**：后置 fallback（`is_edge_csp_capable` 排他门控，只对规则 ⊆ {ring,brick,watchtower,compass,
   inequality,difference,area,precise,range} 触发），插在 aog/rose 之后、pieces 之前。`solve_edge_csp`
   返回 None 走回退（不 `return build_solution` 吞兜底）。`RustSolver.RUST_PARTS` 3→4（子进程墙钟覆盖
   4 段 unit 预算）。
4. **未做（迭代二）**：compass 方向计数 / watchtower / differentiation / fence / solitary 传播、ring OOM
   前置拦截（`is_edge_csp_preempt` 已定义未接入）。

**验证**：`cargo test` 20、`pytest` 290 通过。**结果**（全量 `benchmark_rust_solver.py --timeout 40 -j 8`，
1258 题）：**1072 / 1258**；**14 道新解出**（全过独立验证，`solver=edge_csp`）——0421/0507/0592/0637/0638/
0894/0979/1131/1132/1134/1382/1400/1404/1411（difference/inequality/ring 系，aog 40s 超时后 edge_csp
<13s 解出）。较 bd2f5f5 基线净 +20（14 edge_csp + 7 前序 aog 修复 − 1 flake）。**1333（rose+range，无
edge 规则）PASS→FAIL 与 edge_csp 无关**（`is_edge_csp_capable` 不触发，flake）。

---

## 附录

### A. 当前 DIFF（解 ≠ 官方解）
1. ~~**watchtower DIFF —— 6 道**~~ **已解决（2026-08-06）**
```
Zone3/3-vertex-radar/0543  0544  0662  0663  0800
Zone3/7-zone3-mixed/1144
```
**根因**：官方题在**外边界顶点**上也有望塔，但转换器（`convert_archive.py` 只收集内部
行/列）与模型（顶点数组是内部 `(h-1)×(w-1)`，`build_puzzle` 拒绝边界坐标）**双双丢弃
边界望塔** → 盘面约束不足 → 求解器解出非官方解。**修复**：顶点约定改为**绝对网格坐标**
（`0..=h × 0..=w`，含边界角点），转换器收集全部边界望塔，85 个 watchtower 谜题 JSON
迁移。6 道 + 0985 全部不再产生「合法但 ≠ 官方」的解（6 道解出官方解；0985 加约束后
搜索变难，30s 超时——仍是 FAIL 但**不再是错误解**）。详见第二部分对应条目。

> 曾把 1301 误列入「孪生解」，实为 **brick 规则语义 bug**：`validate.rs` / `IndependentValidator` / backtrack 对含 blocked 的顶点跳过 brick 检查，放过 1 blocked + 3 区域的真 4 路交叉，导致单点 `(7,6)` 的错解被判合法。修复砖纹语义后 1301 唯一解 = 官方解 `(6,7)`（2026-08-06，见第二部分）。

> 曾把 1301 误列入「孪生解」，实为 **brick 规则语义 bug**：`validate.rs` / `IndependentValidator` / backtrack 对含 blocked 的顶点跳过 brick 检查，放过 1 blocked + 3 区域的真 4 路交叉，导致单点 `(7,6)` 的错解被判合法。修复砖纹语义后 1301 唯一解 = 官方解 `(6,7)`（2026-08-06，见第二部分）。

### B. 当前 UNSOLVED 分析（求解器解不出，非错解）

**最新（2026-08-07, commit `6169df3`）**：209 FAIL（79 超时 + 108 无解 + 7 校验失败 + 15 OOM）。

按类型（近似）：Zone3/7-zone3-mixed 33、Zone3/2-loopy 31、Zone3/6-compass-main 29、
Zone3/5-inequality 19、Zone3/3-vertex-radar 18、Zone3/8-endgame 18、Zone3/4-difference 15、
其余 Zone1/Zone2 散布。

优化后（vs 基线）：**8 道新解出**（1270/0710/0749/1329/0875/0795/0829/0957），
**0 回归**。剩余 FAIL 根因：compass/rose/ring 强规则组合搜索空间大、剪枝不足；
fence/non_block/solitary 等规则在 backtrack 中仍为事后检查而非搜索约束。

**fence 搜索中增量剪枝（2026-08-07，专用求解器第一波 #1）**：新增 `solver/fence/` 模块，
`check_fence_patterns` 作为无状态守卫挂入 backtrack `dfs` 守卫链（仿 `check_sealed_regions`），
`has_fence` 门控零开销。4 边界位全定时做 dihedral_key 比对 + arm-count 部分检查（未全定也剪）。
vs `6169df3` 基线：**0 回归，+1 PASS（0829）**；**8 道校验失败 → 0**（backtrack 不再产出 fence 错解，
失败模式转无解/超时/OOM——正确性修复）。fence 子集 171 题 PASS 数未变（127→127），
搜索空间仍太大；下一步拟叠加 fence 边界预推导（k=0/4 全定 + k=2 对臂已知 1 边即全定 + 外边界级联传播）
+ NonBoundary DSU 合并（确定的非边界两端格并为原子单位，缩小搜索空间）。

0. ~~**评估 Python 求解器去留**~~ **已完成（2026-08-06）**：评估证明 Python 求解器对官方语料无解出价值（历史仅解 5 道且现全由 Rust 解出；206 道失败题定向扫描 Python 0 命中）。已删 Python 求解算法、`default_router` 改 Rust-only、保留 constraints/shapes 共享层与 IndependentValidator，测试与文档同步（见第二部分 C.0 条目）。
1. ~~修 Rust **brick 回溯短板**（0957/1301）~~ **已完成（2026-08-06）**：砖纹语义修正 + 删除 `check_merge_ok` + area 剪枝，1301/0957 均由 Rust 解出。下一步可做 **Rust-only 全量回归**（router 只走 RustSolver 验证全部官方题），通过后再评估删 Python 求解器（与 C.0 衔接）。
2. 修回溯内存泄漏（`backtrack._solve_rose_parallel` 守护线程不退出，全量 verify OOM / 1004 300s 不收敛）——全量回归阻塞项。
3. ~~甄别 6 道 watchtower DIFF~~ **已完成（2026-08-06）**：边界望塔缺失（转换+模型 bug）
   已修复，见第二部分。
4. compass / ring 组合剪枝；0446（DLX 形状去重）、1109（compass 专项）、1004（rose+watchtower）。
5. 每次优化后重跑全量扫描刷新「第一部分」数字。

### 2026-09-09 · 1120 官方题里程碑（PR#64 `a04a4ad` = `7c05d41` + `de1f3f4`）
1102 后的收口冲刺，全量 1258 题基准 1120/1258（0 真回归），合并入 `main`（PR#64）。

- **aog rose-cap 预算 3s→20s（`7c05d41`）**：31 道 rose-capable 题此前 aog 仅 3s 预算即被截断、全部超时（如 0957 aog 空载 1.7s 可解）。放宽到 20s 几乎免费——rose 仅约 9 题会解、最慢成功 4.9s（0833），仍留 ~20s 单元预算；10s 仅 +6、30s 与 20s 同为 +8，**20s 为拐点**。
- **`solve_rose` deadline 锚定自身起点（`7c05d41`）**：原先用 router 全局 `start`，但 router 给 rose 的预算是 `timeout_ms - aog_elapsed`（同全局起点），一旦 aog 吃掉超过 `timeout_ms - rose_ms`，rose 的 deadline 已在过去、0ms 返回——这正是 30s aog 预算让 9 道 rose 解里 6 道回退的根因。`backtrack` 早已正确锚定。总墙时不变（aog_elapsed + rose_ms = timeout_ms）。
- **边 CSP 声音的面积可行性裁剪（`de1f3f4`）**：两组件间一条 Uncut 边合并出的区域尺寸至少 `sz1+sz2` 且须满足各方 max_area；若该最小尺寸已超任一 max_area，则无合法解可 uncut，强 Cut。缩减 area/precise/range 题分支（如 0289 爆炸），**剪掉 0 个合法解**。全量基准 1120 保持、0 回归（3 个临界翻转为 `-j 8` 负载噪声，solo 确认）；新 edge_csp 解出 0651，0980/0685 等 brick/ring OOM 在 aog 未被内存 kill 时也解出。
- **新解**：0213 / 0213nopad / 0856 / 0957 / 0620 / 1386（aog）+ 0439 / 0491 / 0445 / 0651（edge_csp）。
- **结果**：1120 / 1258（89.03%），较 1102 基准 +18、较 1084 基准 +36；0 真回归。二进制 `results/bin/rsolver-7c05d41-linux-x86_64`，基准 `results/bench/20260909_areafeas_full.jsonl`。

### 2026-09-18 · edge_csp 形状同一性传播（same / different / mixed）

1120 基线的 138 道 FAIL **全部带规则**（doc 26 基于 1111 基线的「134 道无规则」结论已过时）。
按规则组合统计，涉及 `same`/`different`/`mixed` 的 FAIL 共 19 道，是仅次于
compass+solitary（11 道）的第二大簇；其中 9 道本就在 `is_edge_csp_capable` 门内
（有 fence/ring/brick 等边规则），另 10 道（rose_window+same、different+rose_window 等）
因 `is_rose_capable` 拒绝 same/different 而只由 aog 尝试。

- **传播实现**（`edge_csp/prop.rs`、`types.rs`、`adapter.rs`）：
  - `check_mingle`（`same`）：按 `check_rule_same` 的**全局**语义（`len(shape_keys) <= 1`），
    不是参考 aog 的仅相邻 mingle。首个密封组件定出共享尺寸 `a` 后：任何生长组件
    超尺寸 / 目标面积≠a / 生长潜力<a 判矛盾；已达 `a` 的生长组件强制封口（生长边 Cut）。
  - `check_mismatch`（`different`）：密封组件 canonical 形状两两互异（BTreeSet）。
  - `check_mixed`（`mixed`）：每条 Cut 边两侧若均密封且 canonical 形状相同即判矛盾。
  - 三者均为 false-negative-only（只拒绝、不产解），声音性由 219 题回归背书。
- **门控放开**（`is_edge_csp_capable`）：所有规则 ⊆ SUPPORTED 且含 same/different/mixed
  至少一条 → capable。这给 rose_window+same / different+rose_window 类题第二次机会
  （aog 满预算失败后 edge_csp 接力），墙钟仍在 `RUST_PARTS=4` 预算内。
- **新解**：0341（different+fence，44ms）、1370（different+fence，722ms）、
  1340（different+rose_window，926ms），均 via edge_csp。
- **证伪项**：select_edge 启发式补全（doc 26 §5.3 的 clue 约束组件 +30 / Slitherlink
  路径端点 +45 / rose 邻近 +80）已实现并实测——0924fix（fence+difference，基线 12s）
  与 0972（ring+rose_window，基线 9.2s）串行复测双双超时，属**真回归**（搜索序改变
  砍掉原本可达的分支），已回退。教训：edge_csp 的搜索序对慢题敏感，启发式调整
  必须逐题验证不能只看 FAIL 集合的净变化。
- **验证**：219 道含 same/different/mixed 的 PASS 题全部复测 0 回归；`pytest` 301、
  `cargo test` 34 通过。全量基准待合入前跑（见软门禁）。

### 2026-09-18 · compass bbox 面积界回填 + solitary 可行集 + pieces compass deadline

背景：compass+solitary 是 1120 基线最大的 FAIL 簇（11 道，全部纯 compass+solitary）。
定向调研（未入库）指出三处缺口：罗盘 `-1` 方向使 `max_area` 无界 → 既有放置枚举/
封口/生长势剪枝全部失效；solitary 只在组件粒度传播；`pieces::compass_rec` 无 deadline。

- **`get_compass_area_bounds` 回填 max**（`edge_csp/prop.rs`）：半平面语义下
  `size = 1 + (n+s) + (e+w) - Q`（Q=象限格，被和重复计数）。下界保持
  `1 + max(n+s, e+w)`（对 Q 最紧）；上界改 `1 + Σ_d (v_d or avail_d)`，
  `avail` 来自静态表 `compass_halfplane_avail`（`Solver::new` 一次算好，
  避免每搜索节点 O(HW)）。原先仅四方向全已知才有 max → 含 `-1` 的线索
  （全语料 971 个罗盘线索中的大多数）拿不到任何面积上界。
  **够格进入放置枚举的线索 58 → 155**。
- **solitary S5 可行集**（`propagate_solitary`）：`solitary` 下每区恰一线索，
  故格只能属于「bbox 覆盖该格」的线索所对应的区。bbox 紧性依赖**区域连通性**：
  要到达线索以北第 k 行必须穿越 k 个北半平面格，故 `up==v` 时区域格不会比
  v 行更北（其余方向同理）。S5a 无候选格→矛盾；S5b 相邻格候选不交→强制 Cut；
  S5c 含线索 i 的组件内出现不在 i bbox 的格→矛盾。仅当**全部线索均为罗盘**且
  K≤64（u64 位掩码）时激活——否则 bbox 之外的格仍可属于无线索界的符号/面积区。
- **`pieces::compass_rec` 加 deadline**：某方向为 `-1` 时 `max_sz=None`，递归
  无尺寸上限；原先 0312/0680 会跑到 harness 墙钟被 SIGKILL（attempt trace 为空）。
- **新解**：1386（compass+rose_window）、0418 / 1140fix（compass+watchtower），
  均 via edge_csp、默认单元预算内解出（非加时收益）。
- **未解出**：compass+solitary 簇 11 题全部仍超时。以 1017 为例，bbox 交集在根层
  唯一锁定 7 格（与官方解一致），但这些唯一归属格之间**没有候选不交的邻接对**，
  S5b 无边可强制；需要更强的「唯一归属 ⇒ 到线索的路径必 Uncut」连通性推理
  （类似 dual_connectivity 的桥分析）才能坍缩搜索空间。
- **回归**：154 道 compass / solitary PASS 题全部复测 0 回归。

### 2026-09-18 · edge_csp dual_connectivity D1/D2（structural_pieces via precise）

doc 26 列 loop_closure / dual_connectivity 为 P1，两者都以精确片数为入口门。
doc 27 已证伪玫瑰窗推导（写入 `exact_piece_count` 即打开 two-piece parity
seeding，根层强制错误 Cut）。本次改用**结构规则**来源：

- **`structural_pieces`**：`precise` 规则给出每区面积 `A`，可填格数 `F`，
  则片数恰为 `F/A`（仅整除时设置）。与玫瑰窗路径完全解耦。
- **D1 单生长边强制**：组件必须生长（尺寸 < 目标或 < `curr_min_area`）而
  Unknown 生长边恰 1 条 → 该边必 Uncut（组件别无通路长大）。
- **D2 组件图连通分量**：组件为点、跨组件 Unknown 边为边。每个连通分量
  至少成 1 片，故 `cc > pieces` 矛盾；`cc == pieces` 时每分量恰成 1 片，
  其内部 Unknown 边全 Uncut。
- **D3 桥分析未移植**（风险最高，需论证桥两侧面积，暂缓）。
- **新解**：0209（precise+ring，edge_csp 15.7s）、0703（fence+precise，1.8s）。
- **回归**：169 道 precise PASS 题全部复测 0 回归。1248 仍超时。

### 2026-09-18 · 全量收口基准 1127/1258（`65d2336`）

`--timeout 40 -j 6`，较 1120 基线净 **+7**（9 新解 / 2 损失）。

- **新解 9 道**：0341 / 1370 / 1340（形状同一性传播，见上）；0209 / 0703
  （dual_connectivity，见上）；1386（compass bbox 紧界）；0956（aog 35s）、
  1131（edge_csp 65s）为跨运行临界题，本轮剪枝收紧后在预算内解出；
  0418 本轮 -j 6 争抢未进榜，**串行复测 SOLVED**（edge_csp 27.5s）。
- **损失 2 道**：0685 串行复测 SOLVED（aog 20s，争抢噪声）；0491（watchtower
  8×10）在本轮与 `results/bin/rsolver-7c05d41-linux-x86_64`（1120 提交的二进制）
  上**均 rc=137 OOM**，属固有内存不稳题，非本轮回归。
- **判噪声口径**：与 `docs/code-complexity-report.md` §7.5 一致——并行下 40s 边界
  题随机翻转，逐题串行复测才能定性。按此口径真实能力约 **1129**（+0685 +0418）。
- **产物**：`results/bench/20260918_65d2336_shape-identity-compass-dual.{txt,jsonl}`、
  `results/bin/rsolver-65d2336-linux-x86_64`。

### 2026-09-18 · 最终基准 1131/1258（`c58054d`，回退 S5d 后）

`--timeout 40 -j 6`，较 1120 基线净 **+11**（12 新解 / 1 损失）。

- **S5d 回退**（`c58054d`）：solitary 的「单候选邻接强制」两种形式均被 1017
  证伪——格级版忽略组件可经其它边生长（1017 格 (5,0) 自身边全已 Uncut 仍
  被判矛盾）；组件级版修正后仍在 4s 内耗尽。S5d 只在 compass+solitary 题上
  触发，该簇 11 道有无 S5d 均未解出，回退不影响任何增益。保留 S5a/b/c。
- **12 新解**：0341 / 1370 / 1340（形状同一性传播）、0209 / 0703
  （dual_connectivity D1/D2）、1386 / 0418（compass bbox 面积界）、
  0956 / 1131 / 1140fix / 0990 / 1146（临界题受益于剪枝收紧，aog 或
  edge_csp 在预算内解出）。
- **1 损失**：0491（watchtower 8×10）在 `results/bin/rsolver-7c05d41-…`
  （1120 提交的二进制）上同样 rc=137 OOM，属固有内存不稳题。
- **产物**：`results/bench/20260918_c58054d_shape-identity-compass-dual.{txt,jsonl}`、
  `results/bin/rsolver-c58054d-linux-x86_64`。

### 2026-09-18 · 本轮收口 1131/1258（`8221be7`）

`--timeout 40 -j 6`，较 1120 基线净 **+11**（12 新解 / 1 损失）。这是本轮
`feat/shape-identity-propagation` 分支的最终数字。

- **12 新解**：0341 / 1370 / 1340（edge_csp 形状同一性传播）、0209 / 0703
  （dual_connectivity D1/D2，structural_pieces via precise）、1386 / 0418
  （compass bbox 面积界回填）、1433（rose_window 单独可进 edge_csp 门控）、
  0745（pieces）、0956 / 1131 / 1140fix（多处剪枝收紧后的临界题）。
- **1 损失**：0491（watchtower 8×10），在 1120 提交的二进制
  `results/bin/rsolver-7c05d41-…` 上同样 rc=137 OOM，属固有内存不稳题。
- **本轮证伪/0 增益项**（均已回退或降级为基础设施，详见 doc 27/28 与记忆）：
  select_edge 启发式扩展（真回归）、exact_piece_count 玫瑰窗推导（阻塞
  loop_closure）、solitary S5d/S5e 连通推理（1017 证伪）、AOG_SHAPE_CAP
  各档（净负）、check_complement_feasibility（0 新解，已回退）、
  rose_growth 连通/边界守卫（正确性修复，0 PASS 增益）。
- **产物**：`results/bench/20260918_8221be7_final.{txt,jsonl}`、
  `results/bin/rsolver-8221be7-linux-x86_64`。

**距 1140 目标还差 9 道。** 剩余 127 道 FAIL 的主要簇：compass+solitary 11
（三种传播方案均被证伪，实测 90s 预算也仅 1~2 道可解且引发 OOM）、rose 约 20
（greedy 范式解不出，非法候选已修）、brick+ring OOM 约 8（aog 形状库爆炸，
cap 各档净负）、edge_csp 模型缺口约 22（exhausted，多为 fence/non_block/
watchtower 组合，基线即如此）。下一步需要范式级工作（doc 20 的 rose→edge_csp
边传播迁移，或 aog 形状枚举根治）。

### 2026-09-20 · bricky_loopy 赌博剪枝修复 + OOM 兜底重试 → **1146/1258（doc 29）**

`--timeout 40 -j 6`，较 1130 基线净 **+16**（17 新解 / 1 损失）。**1140 目标达成。**

- **主修复（声音性）**：`propagate_bricky_loopy` 的 ring+brick 与 bricky-only
  分支把「至少 n 条 Unknown 须 Uncut」实现成「强制**前 n 条** Uncut」——挑边是
  赌博。1378 上根层（nodes=0）误强 cell (2,2) 的 W/E 为 Uncut，palisade 按
  fence_pattern=Three 判矛盾，官方解被剪。诊断：`EDGE_CSP_DEBUG` + propagate()
  临时 step! 宏标注 Err 来源 + 官方解逐边比对。改为仅当**全部** Unknown 都必须
  Uncut（`cut_count == 2` / bricky `== 3`）才强制，中间情形只做矛盾检查。
- **新解 9 道 ring+brick 簇 via edge_csp**：1373 / 1374b / 1375 / 1378 / 0834 /
  0631 / 1110 / 0977 / 0978（该簇此前 14 道 FAIL 的共同根因）。
- **OOM 兜底重试**：`RustSolver.solve` 在子进程 exit -9 时以
  `AOG_SHAPE_CAP=200000` 原预算重试一次。默认路径 cap 保持 0——实测 0710
  （area 8×8）合法形状库超 10M 条目，任何全局 cap 都会回归它；0384/0870/1131
  在 200k 下亦回归、2M 下恢复。兜底让 1373/1375/0834/0977/0978/0969 等
  aog OOM 题在重试中由 edge_csp 接住。
- **1 损失**：0745，串行复测 SOLVED（pieces 40s 临界，-j 6 争抢噪声）。
- **同日二次证伪**（doc 27 §5.1）：exact_piece_count two-piece parity seeding
  在 0974（ring+rose、无 vertex 线索）上 nodes 1173→34 错剪——seeding 不可靠
  不依赖 watchtower 上游错误边状态。loop_closure 挂 structural_pieces 的移植
  实验（0974 nodes 减半仍超时）收益不足，未合入。
- **产物**：`results/tmp/20260920_bricksound_oomretry.jsonl`（全量）、
  `results/tmp/20260920_bricksound.jsonl`（无兜底对照，1137/1258）。
- **测试**：`pytest` 301、`cargo test` 34、`complexity_gate.py` 全过。

**剩余 102 道 FAIL 的主要簇**：compass+solitary 约 11、rose 约 20（greedy
范式缺口）、watchtower/difference/inequality 约 22、无规则纯分区约 50。
下一步（新目标 1176）：doc 20 rose→edge_csp 边传播迁移、doc 26 未吸收技术
（probing 增强 / mingle_shape / complement_feasibility 重估）、watchtower
大题的 config 枚举扩展。

### 2026-09-22 · same-tiling 同形铺砌预通路 + pieces/pp-pin/rose 改进（doc 12，→ 1172/1258）

- **动机**：`same`/同形特例（0382/0763/1098 簇）在原路由下全灭——`is_rose_capable`
  明确拒绝 `same`/`different`、aog 形状库在开阔区爆炸、edge_csp 形状同一性传播只
  能叶验不能搜索、pieces DLX 没有「自由同形分区」概念。
- **新模块 `solver/same_tiling.rs`（⓪ 预通路，`docs/rust-solver/12-same-tiling求解器.md`）**：
  ① 循环等距窗口 CSP（`F = S ⊎ ψ(S) ⊎ … ⊎ ψ^{m-1}(S)`，滑窗恰一系统按链求解；
  `inv` 忘乘 det 曾把镜面映到错误逆元，0382 类全灭）；② m==2 连通横截生长（ψ 对合
  完美匹配，自配对必须拒绝）+ **m==2 自由尺寸生长**（无等距假设：S 连通生长、补集=T）；
  ③ 小形状 DLX（s≤8 自由 s-ominoes × dihedral 落点）；④ shape_pattern 预钉 × 同形余数
  （整除门只门控全局同形法——预钉情形各区域尺寸不等）；`derive_region_count` 四源
  （rose 符号数 / solitary / precise / area_sum 去重值和）。
- **m2 自由生长剪枝**：fence 星形臂一致性（局部 4 边界位 vs dihedral 星构型）、必分边
  （boundary/constraint 两端必异区）、二区望塔界 `[d, d+min(u, 2-d)]`（**修复**：原多区
  语义 `lo=d+1` 要求未决格各成新区，1137 的 80 个望塔在根层全灭整棵树）、T-closure。
  **遗留（下轮 P0，m=2 簇 5 题的根因）**：① T-closure 不健全——要求全部非 S 格可达 T
  种子，但未决格本可并入 S，S' 成「墙」时误杀官方分支（正确语义：仅 label=1 格必须
  可达，不可达的未决格应强制并入 S）；② fence 星与必分边只有检查没有单位传播（m=2
  下必分边即 XOR 强制，一条边定另一端）；③ `visited.len()>=800_000` 上限 abort 被报成
  exhausted（AOG_DEBUG 实证 1249/1137 均在 780k→780k 撞顶）。0974（46 预切、无
  fence/watchtower）进不了 local-density 门，需把预切计入密度。
- **pieces 三修**：compass 半平面计数（原 N>S>E>W exclusive，与 validator 的象限双方向
  半平面计数不符——**0/103 罗盘题经 pieces 曾全灭**）；`has_constrained_compass` 门控
  放宽到任意单方向约束（原 `spec>=3 || 双零 strip` 把 0418 这类 left-only 线索全判
  「无约束」，pieces 罗盘路径对它们从未点亮）；frontier+visited 完备枚举（共享
  candidates 的 DFS 漏放置，0763 的官方放置缺失）；叶覆盖检查只看可填格（blocked 的
  `None` 是合法状态，曾误拒 0763 全部 24/24 合法覆盖）。新增 block 矩形落点、
  shape_pattern 落点、`rose_signature_ok`（每放置每种玫瑰符号恰一）、row_check 分解
  （watchtower/econs/distinct/cell-clue/compass）。
- **pp-pin**：`combine_plain` 重写为锚点覆盖式（一放置可盖多同形锚，消除共享
  `candidates` 漏解 bug 类）；same-class 锚检查移除（0976 教训：干净路径被砍，叶验
  `check_puzzle_piece` 足够）；`watchtower_facts` 提为 `pub(crate)` 与 same-tiling 共享。
- **预算/稳健性**：rose 改满单位预算（原 `timeout_ms.saturating_sub(elapsed)` 被 aog
  超支饿死至 0ms 跳过，0382 类）；`prop.rs` probe 超时护栏（deadline kill ≠ failed
  literal，0312 搜索树确定化）；`RUST_PARTS` 4→6（same-tiling/pp-pin 各占单位预算）。
- **新解 +13（串行验证，via 归因）**：1183（area_sum）、0382/0383/0763/0960（same-tiling
  2-18ms）、1098/1099/1100（预钉余数 0.2-3s）、0493（pp-pin，25s 150 万叶 → 33ms）、
  0223/0826（pieces 枚举 deadline+签名过滤，40s 超时 → 3ms）、1004（block DLX 74ms）、
  0745（pattern 落点 6ms）。全量 `-j 6 --timeout 40` **1172/1258**（同口径 vs 1162 净
  **+10**；3 道历史争抢噪声题翻负未进榜）。
- **测试**：`pytest` 301、`cargo test` 36+8（`clamps_zero_to_floor` 为既有并行 env 竞态
  flake，单跑通过）、`complexity_gate.py` 三关全过（m2_region_growth 拆出 growth_facts）。

### 2026-09-22 · m2 自由生长传播化二轮（XOR/星形 AC/望塔关系）——簇仍未破，范式边界实证

- **实现**（`same_tiling.rs`，`docs/rust-solver/12` §3.2b）：`propagate_labels` 不动点
  ——① **XOR 关系传播**：必分边 + rose 符号对（每区恰含每符号一个 ⟹ 同型两格必异区）
  统一成 `force_relation` 二元差/同关系，一端定另一端；多类型 pin 修正为只钉一个根格
  （原 per-type 首格钉 S 在 ≥2 类型时漏掉混合配对——全局对换只容一个代表）；② **fence
  星 AC**：构型域按已知边界位过滤后**按臂取共识**（全部幸存构型同值即强制，唯一构型
  是特例），相邻 fence 星共享边（南臂=邻居北臂）互相收窄构型域，共识臂反推标签关系；
  ③ **望塔关系**：val=1 全等（EQ 星）、val=2 两格顶点=XOR 对、其余全定同标签时末格翻转
  （计数界仍在 `growth_prunes_ok`）；④ **T-closure 健全化**：只有强制 T 格须同属一个
  非 S 连通分量，**不可达 T 的未决格强制并入 S**——原版要求全部非 S 格可达 T 种子，
  S' 成「墙」即误杀官方分支（一轮根因）；⑤ 传播写入按分支快照回滚、visited 键改传播后
  S 集、状态 cap 800k→2M（到顶是预算结果非无解证明）。
- **门控**：`has_constrained_compass` 放宽到任意单方向约束（spec≥3 曾把 0418 类 left-only
  线索整题拒之门外，pieces 罗盘路径对它们从未点亮——25 道 compass FAIL 题由此进门，
  0 新解但不再 not_attempted）；same-tiling 入口与 m2 密度门**计入预切/约束边**
  （0974: 46 预切由此进门）；precut-only 进门后只跑 m2，不再烧整除门后的等距法。
- **结果**：外围 7 题（0382/0383/0763/0960/1098/1099/1100）零回归；**m=2 簇 5 题仍未解**
  ——1137/1149a/0974/1249/0987 全部 40s deadline 打满（传播显著压树：从「800k cap 秒退」
  到压树后仍爆炸；1249/0987 纯 fence 无预切无望塔，格枚举树天然巨大）。**结论：cell-
  variable 连通子集枚举与 fence 星 / 望塔 / 预切这类边界几何线索范式错配**（约束描述
  割边结构而非格归属），与 docs/优化/18、20 判断一致；格变量剪枝收益已尽，簇的正解是
  边变量宿主（edge_csp 的 palisade/watchtower/parity 传播 + 边 DFS 覆盖 rose m=2）。
- **快速档**：`results/bench/20260922_47d2e6f_m2prop-fast.{txt,jsonl}` **1170/1175**（REGRESSION=2 均为
  0418/1140fix 历史争抢噪声题，今日串行复测 28.9s/23.0s SOLVED；NEW=0；0 真回归）。
- `pytest`、`cargo test` 36+7（`clamps_zero_to_floor` 既有 flake）、`complexity_gate.py` 全过。

### 2026-09-22 · exact_piece_count 重开 + rose pair 分支声音性修复 + aog 单位预算 → **1176/1258**

- **`exact_piece_count` 重推导**（`edge_csp/mod.rs`，doc 27 §6 更新）：doc 27 的证伪前提
  「two-piece parity 放大根层错强」已被 3262e5d（watchtower Pass B 对角格误判）等修复
  愈合——1135/1392 从历史回归（nodes=0 假穷尽）复测为 **0.3s/1.3s SOLVED**。推导规则：
  rose 各类型格数全相等 n ⇒ `Some(n)`。**Cut 边 parity=1 seeding 是大杀器**：搜索中每条
  已定 Cut 边都是 XOR 事实，0987 由此从 40s 超时变 7s 解出（doc 27 §2 预言的 6.9s 兑现）。
- **rose pair 分支四处修复**（`rose.rs`/`mod.rs`，bug 类见 `docs/优化/27` 附录）：
  ① **记录 vs 强制**：`branch_pair_same` 把「同区」实现成强制一条任意 BFS 路径全 Uncut——
  同区只需*存在*某条路径，强制特定 witness 漏掉「同区走另一条路」的全部解（**不完备分支
  = 假 exhausted**，0974 曾 175ms 假穷尽）。改为记录 SAME 关系（与 DIFF 对称），parity UF
  以 parity-0 消费。② **快照泄漏**：`Snapshot` 只回滚边轨迹，`pair_branch.diffs/sames`
  跨分支永久残留污染后续搜索——restore 现在一并回滚。③ **同对重复分支 → 栈溢出**：记录式
  SAME 后边状态不前进，`select_rose_pair` 不过滤 `same_set` 会再选同一对 →
  `branch_on_pair`↔`backtrack_edges` 无限互递归（1340/1352 **exit -6 stack overflow 秒退**）。
  ④ **pair 分支仅在 parity1 源存在时启用**（two_piece 或 2-of-a-type）：m≥3 且无 2-of-a-type
  时 sames/diffs 无人消费、纯翻倍树（1433/0655 64s 超时 → 修后 **0.5s/1.5s SOLVED**）。
- **aog 单位预算修复**（`solver/mod.rs`）：deadline 原锚全局 start，same-tiling 烧完单位后
  aog 拿 0ms（attempt 链里 `aog timeout 0ms`），违反 RUST_PARTS=6 的单位预算契约；改锚自身
  `Instant::now()`。**+4 道 aog 新解**：0268/0704/0941/0988（均 ~40s 整点，原先预算被前序
  模块吃掉而超时）。
- **新解 +6**：0974（ring+rose 46 预切，via edge_csp 13s）/0987（ring+fence+rose，7.4s）/
  0268/0704/0941/0988（via aog）。**m=2 簇还剩 1137/1249/1149a**（40s 诚实超时，无假穷尽）。
- **全量基准 -j 6 --timeout 40：1176/1258（净 +4）**；2 个翻负（0418/0685）均为历史争抢
  噪声题（今日串行复测 28.9s/20s SOLVED）。`pytest`、`cargo test` 36+7、`complexity_gate.py` 全过。

### 2026-09-22 · loop_closure 选择性移植 + 望塔精确度判死 → **1137 进账（1177/1258 口径）**

- **loop_closure 选择性移植**（`prop.rs::propagate_loop_closure` + `cut_loop_count`，源自
  `third_party/aog/src/solver/propagation/loop_closure.rs`，doc 26 §2.1 的 P1 项在
  exact_piece_count 解锁后落地）：只移植**规则 1**（割边顶点图的闭合环数 ≤ pieces−1，
  **必须挂 ring 门**——brick 允许 3 度 T 连杆，分离的环可经 T 合并，中间态环数不单调；
  1294 根层 brick 强出 5「环」对 max=2 被误杀、官方 3 块解正是靠 T 连杆合并）与
  **规则 4**（`max_loops==1` 且 ring 时，所有触边框顶点的边必 Uncut——单接口曲线
  不能终止于边框）。`max_loops` 来源 = `exact_piece_count ∪ structural_pieces`。
  **参考的 `@` 顶点度数规则刻意未移植**：其假设 watchtower value = 所需割度，而游戏
  语义是**区域去重数**（Python `check_rule_watchtower` 为准）——官方解实测 471 个
  val=2 顶点里 **241 个割度 ≠ 2**（blocked 象限与多色排列），照搬即大面积误杀。
  规则 1（ring 子集）/4 经官方解 0 违例验证后才启用。
- **望塔精确度判死**（`propagate_watchtower_degree`）：4 满象限内部顶点的割度即 4-循环
  染色的转移数——val=1 ⟹ 度 0、val=2 ⟹ 度 ∈ {2,4}、val=3 ⟹ {3,4}、val=4 ⟹ 4；配合
  `max_loops==1`（单接口）val=2 塌缩为**精确-2**（m=2 全体官方解 634/634 度=2；度 3/4
  全部落在 blocked 象限或非 m=2 题）。blocked 象限破坏循环论证（出现奇数度），跳过。
  **KNOWN BUG（留待下轮）**：singleton fit 后的未定边钉死（hi==c→Uncut / lo==c+x→Cut）
  实测杀真解（1135/1392/1137 秒退），纸面与官方解都看似健全但交互 bug 未明——当前
  只保留**矛盾判死**（fits 空 → Err），强制分支留空并附源码注释。
- **新解 +1 via edge_csp**：**1137**（ring+inequality+watchtower+rose，m=2 簇第三员；
  前两员为同日的 0987/0974）。m=2 簇仅剩 **1249**（fence 无 ring，规则 4 不适用）与
  **1149a**（判死后仍超时）。
- **快速档回归**（基线 v7-full 1176）：REGRESSION=1（1294，已定位为规则 1 缺 ring 门
  的误杀并修复；ring 门属放宽剪枝，严格单调不丢解）+ 复测 12 题矩阵全绿（1135/1392/
  1137/0987/0974/1294/1433/0655/1340/1352 SOLVED）。`pytest`、`cargo test`、
  `complexity_gate.py` 全过。

### 2026-09-23 · 1183 全量实测归档 + 进度图补录（1200 轮弹药侦察）

- **动机**：CI「Benchmark & trend」被 runner shutdown 打断（`##[error]The runner has received
  a shutdown signal`，exit 143，非代码问题），README 徽章/曲线停在 `c457981`@1176。本地按同
  口径（`--timeout 40 -j 6`）对合并态 `4a81357` 全量实测 **1183/1258（94.04%）**，经
  `solver_history.py append/render` 补录为第 30 个数据点。
- **+7 逐一对账（vs `c457981` FAIL 集，0 翻负）**：1137/0990/1146/1147/1406 是 8e9d632
  loop_closure+望塔判死的能力兑现（快速档 NEW=0 是基线口径所误，全量实证 +5）；0418/0685
  为争抢噪声带回正。1130 稳定线成员本轮仍翻负（串行 33s SOLVED）。
- **1200 轮侦察结论**（详见主表备注）：compass+solitary 9 簇是最大可攻坚簇。根因 =
  `pieces.rs::generate_compass_polyominoes` 对 `-1` 无界方向的 frontier 生长无 size 上界，
  靠 `MAX_COMPASS_PLACEMENTS=2000` / `MAX_COMPASS_ENUM_STATES=200_000` 硬截断，真解放置被
  截掉后 DLX 报假穷尽（`pieces:exhausted`）。**参考 C++ AoG_Solver 对照实测同超时**（1246
  60s / 0312 30s 零输出；对照题 1283 秒解，harness 无误）——参考实现盲区，正解是新算法
  （solitary 锁 k=#compass 线索数的 k 区域联合划分搜索：紧线索先生长、松线索吃残余 + 全线索
  半平面增量计数 + area 联合界 + 连通可达剪枝），非移植。

### 2026-09-23（下午） · compass-part 落地（doc 13，+5）+ wdegree KNOWN BUG 侦查

- **compass-part（路由 ⓪b，`65c32e4`）**：`{compass, solitary}` 纯净题联合划分搜索，
  细节与声音性教训见 `docs/rust-solver/13-compass划分求解器.md`。新解 **0312/0680/
  0681/1246/1259**；0682/0683/1258/1260 留待 cell-labeling CSP v2（首块松线索格网
  >20M 状态，官方前缀二分实证只缺首块枚举，后段推理秒解）。
- **wdegree 强制分支 KNOWN BUG 侦查**（强制保持关闭）：
  1. **模型层完全健全**：val→割度四前提对 1568 个官方满象限顶点 **0 反例**；
     max_loops==1 塌缩（m=2 val=2 → deg==2）634/634；ring 无奇割度 587/587。
  2. **追踪法陷阱**：`#[track_caller]` 边写入追踪会把 **probe 沙箱的试探性假设**
     （`self.probe(|s| s.set_edge(...))` 闭包内）记成强制提交——据此得出的
     "根层错误强制" 全部是假象；判定演绎错误必须按调用行排除试探写入。
  3. 放大器结构与 doc 27 parity 同类：输入边状态一旦有误，singleton-fit 强制级联。
     上游嫌疑收窄到 probe 沙箱恢复后的**陈旧组件缓存**（`snapshot()` 只回滚
     edges/pair_branch，`curr_comp_*` 不在快照内）——待证，见 doc 27 追记。

### 2026-09-23（夜） · wdegree KNOWN BUG 结案（doc 27 §8，`ed2e4a3`）+ 进度图抽稀（`c89c6b6`）

- **真凶**：陈旧组件缓存喂给 `rose_separation` 卡口 BFS 的假 Uncut 强制；
  wdegree 强制只是写流量放大器（诊断法：probe 沙箱深度标记 + 按调用行过滤
  试探写入 + skip 门三分——`skip=rosesep` 即活，一路锁到 `rose.rs` 卡口强制点）。
  上一条「上游嫌疑收窄到陈旧组件缓存」的方向正确，但对象不是 snapshot 漏字段
  （回滚机制无罪），而是**消费端不重建**。
- **修复**：谁读谁重建（watchtower / rose_separation / rose_phase3 入口
  `build_components()`）。singleton-fit 强制解锁后 1135/1392/1137 全 SOLVED
  且 1137 提速（29s→24s）。
- **进度图**：横坐标标签抽稀（PNG ~8 刻度 + 45° 旋转、SVG ~10 刻度，hover
  保留逐点日期）。
- 口径维持 **1188**（1183 全量 + compass 簇 5）；下一步 m=2 配套
  （1149a/1249）与 compass-part v2（0682/0683/1258/1260）冲 1200。

### 2026-09-23（深夜） · compass-part v2 cell-labeling CSP（doc 13 §6，+2 → 1190 口径）

- **范式翻转**：v1 逐区域集合展开死于首块枚举（松 `-1` 窗口），v2 改
  格→区域标签 CSP。传播五件套：势连通域（多源 `u128` 位集 worklist）、
  半平面基数 AC（过近似池 + 恰等强制）、**象限联合计数**（象限格同时供两方向、
  精确小目标被超额满足——逐方向池检查看不见的矛盾类别；无-x 可行性只依赖
  8 分区，n×k→8×k 后 3.1× 提速）、尺寸窗精确覆盖、可桥接性（叶子即最终连通性）。
- **搜索机制教训（向导统计法入档）**：给官方解做「向导搜索」+ 逐步打印值序排名
  是定位爆树病灶的利器。三轮值序实验实证：①首选值几乎全对（avg-rank 1.04）但
  DFS 病理在**错误分支死得慢**，根节点一处误排烧穿 4M 帽；②「最松优先」被
  永久松动的残余-eater 标签一票否决；③S1「j2/j5 对称」与 S31「蛇尾远端」全是
  MRV 跳跃取格的人工产物——**前沿取格 + 邻接优先**后 0682 直接解出。单个启发式
  互有反例，只有「前沿取格 + 邻接优先 + 启发式仅排序不剪枝」的组合稳定。
- **两阶段入口**：v1 短缰绳 `timeout/3`（8–12s）防松题磨光预算饿死 fallback，
  v2 吃剩余（同一 Model）。v1 簇 5 题 12s 内零回归。
- **实测**（RSOLVER_TIMEOUT_MS=40000 二进制直跑）：0312 0.16s / 0680 0.23s /
  0681 0.14s / 1246 4.3s / 1259 <0.01s（v1 簇）+ **0682 17.4s / 1260 12.9s（新解）**；
  0683（19M 节点 60s）/ 1258（k=44，1.2M/60s）未解——deadline 截断非穷尽，
  搜索空间/启发式残余（审计测试证传播健全），`#[ignore]` 跟踪于 doc 13 §6。
- 口径 **1188 → 1190**（1183 全量 + compass 簇 7）。冲 1200 剩余弹药：m=2 配套
  （1149a/1249，+2~4）、0683/1258 残簇（+2）、OOM 0224/1215（+2）。

**追记（值序次键配额紧度）**：向导统计实证配额紧度（`pool−short`）**只能当次键**——
主键三题误排全面变差（17→26 / 22→40 / 42→73），次键（邻接主键+配额破平）最优：
0682/0683 max-rank 5/7→2/4、1258 avg 2.89→1.46。残簇未破（0683 14M/60s、1258
5M/60s，deadline 截断）：剩余瓶颈是**错误子树死亡率**而非偏好精度（一处 rank-1
误排的错误子树即吞百万级节点）——下一迭代方向为 joinability 构造化下界（桥接
0-1 BFS 距离储备）与关节格强制（doc 13 §6.2/§6.3）。

### D. 软门禁（Soft Gate）
对以下任一模块的**每次优化**（修复、性能、规则语义、转换），提交前必须：
1. **本文件**：第一部分（进度快照）与第二部分（变更记录）各追加一条。
2. **相关文档**：`faq.md` / `rules-guide.md` / `architecture.md` 等，凡涉及处同步。
3. **README**：若影响外部可观察行为（命令、规则数、已知限制）同步。
4. **测试**：`pytest`、`cargo test`、相关 `benchmark_rust_solver.py` 片段，把结果记入本文件。
5. **归档 artifacts 随提交入库**：影响求解结果（可解性 / 性能 / 规则语义）的提交，必须把对应基准
   输出存为 `results/bench/<日期>_<commit-id>_<short-message>.txt` 并**随该提交一起入库**（不允许
   只留在 /tmp）；临时验证 / 分析输出放 `results/tmp/`。同时把产出该结果的 `rsolver` 二进制存为
   `results/bin/rsolver-<commit-id>-<platform>`（如 `rsolver-f1cfa16-linux-x86_64`，结果可复现）。
   规则见 AGENTS.md「results/ 目录规则」。纯文档、无行为变化的重构等不影响求解结果的提交可豁免。

不满足即视为未完成，不应合入。
