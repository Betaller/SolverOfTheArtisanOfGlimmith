# Bug 跟踪

本目录用于持续跟踪 TAGSolver 中发现的隐藏 bug。每轮扫描结果写入独立的带日期报告，本文件作为索引与状态看板。

## 报告索引

| 日期 | 报告 | 提交 | 范围 | 数量 |
|---|---|---|---|---|
| 2026-08-29 | [2026-08-29-bug-scan.md](2026-08-29-bug-scan.md) | 8c9da3d | rsolver/src + src/ + web/ | C3 · H5 · M2 · L7 · W4 |

## 状态看板（按严重度）

图例：✅ 已修 · ⏸ 已修但实测负收益→回退（修复与证伪都留在分支历史中） · 🔴 待修

### CRITICAL（用户可见错误输出 / 数据损坏）
- ✅ C1 `src/ui/constraint_panel.py:251` set_puzzle 重复追加 Rule — PR #57
- ✅ C2 `src/ui/main_window.py:648` 求解中换题旧解画新盘 — PR #57（求解 token + 换题取消）
- ✅ C3 `src/ui/constraint_panel.py:231` 规则 params 空导致静默不可解 — PR #57

### HIGH（Rust soundness，被 Python 独立校验兜底但丢解）
- ⏸ H1 `aog/empty.rs:244` empty_block_line_count 过度剪枝 (UNSOUND) — 精确解（最小顶点覆盖=最大匹配，Kőnig）
  已实现并回退：该过度剪枝是 aog 搜索性能的承重墙，松弛后搜索爆炸，slash-pack `0833` 挂死 >50s。
  回退于 `7ac26cf`；理论上的丢解风险仍在（见 `fix/rust-solver-soundness` 分支历史 d037fdd）。
- ✅ H2 `aog/core.rs:919` block 矩形目录漏竖长矩形 — PR #58
- ✅ H3 `aog/search.rs:1311` block 未在形状池路径强制 — PR #58
- ✅ H4 `edge_csp/prop.rs:1349` 桥接迫使 Uncut 过度 (UNSOUND) — PR #58
- ✅ H5 `mod.rs:344` build_solution_trusted 跳过 validate — PR #58

### MEDIUM
- ⏸ M1 `rose/region_match.rs:107` 多符号 rose 早停剪枝 — 移除早停后填充超集淹没 `CANDIDATE_CAP`，
  `0833` 由可解（~7s）变无解且 NEW=0。回退于 `f79c51d`；理论丢解风险仍在。
- ✅ M2 `rose/*` `1u32<<n`(n≥32) UB — PR #58（掩码改 u64；>64 种符号显式拒绝）

### WEB（Vue3）
- ✅ W1 `web/src/App.vue` 全局 keydown 劫持文本输入 — PR #55
- ✅ W2 `web/src/components/GridCanvas.vue` 多位数字跨格泄漏 — PR #55
- ✅ W3 `web/src/components/PropertyPanel.vue` 数字 0 被静默丢弃 — PR #55
- ✅ W4 `web/src/components/ConstraintPanel.vue` 参数化规则默认未落库 — PR #55（LIKELY 已确认）

### LOW（鲁棒性/时效，不影响答案正确性）
- ⏸ L1 `aog/empty.rs:342` deadline 检查死代码 — 检查改为每轮后关闭了一个“deadline 盲区”，
  该盲区目前恰是通过若干题的路径，改动导致回归。回退于 `add102e`（优先级低）。
- ✅ L2 `src/ui/main_window.py:340` 撤销栈存编辑后状态 / redo 误清 — PR #57
- ✅ L3 `src/ui/grid_widget.py` 填数等不 emit board_modified — PR #57
- ✅ L4 `src/ui/solver_runner.py:25` cancel() 空操作 — 已在 HEAD 上游修好（router.cancel → 杀子进程），无需改动
- ✅ L5 `src/models/board.py:201` clone() 丢 outer_boundaries — PR #57
- ✅ L6 `src/solver/rust_solver.py:300` solve_batch stderr 未排空 — PR #57（改 DEVNULL）
- ✅ L7 `src/ui/puzzle_browser.py` basename 键 / 裸 except — PR #57

### 本轮结论（2026-08-31）
- 21 项中 **16 项已修并合入 main**（PR #55 / #57 / #58），1 项（L4）上游已修。
- **3 项（H1 / L1 / M1）修复已实现但实测净负收益，回退并保留在 `fix/rust-solver-soundness`
  分支历史与上述说明中**——三者的共同模式是“修复在理论上正确，但被移除的行为是性能承重墙”。
- 门控口径：对 main 基线 `results/bench/4be9922_full.jsonl`（1088/1258）全量复测，
  最终 REGRESSION=0 / NEW=2（`Zone3/0418`、`Zone3/1405`）。
- ⚠️ 复测教训：`-j 8` 下 `0685/0956/0957` 会假报回归（均 ~20-24s 的临界解），solo 重跑全部通过；
  且**机器负载必须先确认**（本次因残留 rsolver 孤儿进程导致 load≈36，曾产出 27 条假回归）。

## 约定
- 新发现追加到最新日期报告；若跨多轮，新建 `YYYYMMDD-bug-scan.md` 并更新上方索引与本看板。
- 每条 bug 带 `file:line`、复现场景、为何错误；修复后在状态看板把 🔴 改为 ✅ 并注明提交 sha。
- 临时分析仍放 `results/tmp/`；正式跟踪报告统一放本目录。
