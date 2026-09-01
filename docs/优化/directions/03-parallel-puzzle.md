# 03 · 谜题间并行 + 子进程复用

> 状态：✅ 部分落地 ｜ 分类：并行化 ｜ 来源：`docs/优化/24` §4.3
> 关联：[01-parallel-inter.md](01-parallel-inter.md) · [17-fast-codec.md](17-fast-codec.md)

## 1. 一句话
不同谜题之间天然独立，可多进程并行求解；且单题调用应常驻一个 rsolver 子进程复用，消除每次 `spawn` 的开销。

## 2. 思想（为什么有效）
- 谜题间无任何共享状态，是最廉价的并行（Amdahl 定律下接近完美加速比）。
- 子进程复用（`--batch`）：`benchmark_rust_solver.py` 已支持 `-j N` 多 worker + `--batch M` 单子进程多题（见 `TODO.md` P2 #7，2026-08-06 落地，~6× 吞吐）。但**单题 UI 调用**（`solver_runner`）每次仍 `spawn` 新进程（`rust_solver._find_binary`）——这里可常驻复用。

## 3. 现状与代码位置
- 批量：`scripts/benchmark_rust_solver.py`（`-j 8` 默认 `cpu_count`）、`--batch N`（`rust_solver.solve_batch`）。
- 单题：`src/solver/rust_solver.py:119` `_find_binary` + `solve` 每次新建子进程。
- 协议：`rsolver/src/main.rs` `--batch` 多行 JSON 逐行进出。

## 4. 收益
- 批量基准吞吐已 ~6×（batch 复用）；UI 实时求解若常驻子进程，单次延迟去除 spawn（~10–30ms）开销。
- 多核利用率在批量回归/CI 已拉满。

## 5. 代价与风险
- **风险：低**（已落地部分）。批处理局限：某题超内部 budget 会连带同批后续题超时——`08` 已用 timeout 透传修复（`--batch 1` 精确验证）。
- 常驻子进程需处理「子进程崩溃重启」「空闲回收」。

## 6. 优先级 / ROI
- **已落地**（P2 #7）。剩余补强：UI 常驻子进程池（小改动，UX 提升）。

## 7. 实现思路
- UI 侧：`solver_runner` 维护一个 `RustSolver` 长生命周期子进程（或 worker pool），用 `--batch` 行协议发题收解；崩溃则重建。
- 进程管理：超时/崩溃检测 + `select` 读保护（`rust_solver._BatchLineReader` 已有）。

## 8. 验证方法
- 批量：`--batch N -j 1` 与 `-j 8` 解出数一致（REGRESSION=0）。
- UI：手动手绘大格，观察实时求解延迟。

## 9. 依赖与前置
- 依赖 [17-fast-codec.md](17-fast-codec.md)（换更快编解码可再提吞吐）。

## 10. 参考
- `docs/rust-solver/TODO.md` P2 #7；`docs/优化/24` §4.3；`scripts/README.md`。
