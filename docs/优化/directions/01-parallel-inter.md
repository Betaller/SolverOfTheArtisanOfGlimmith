# 01 · 模块间并发「先赢取消」

> 状态：🟢 新方向 ｜ 分类：并行化 ｜ 来源：`docs/优化/24` §4.1
> 关联：[02-parallel-intra.md](02-parallel-intra.md) · [13-meta-rustparts.md](13-meta-rustparts.md) · [29-adapt-budget.md](29-adapt-budget.md)

## 1. 一句话
五个求解模块当前**串行**且各自独吞完整 `timeout_ms`，但它们彼此**独立、无共享可变状态**——改成「并发跑、先赢者取消其余」即可在**零算法改动**下吃掉串行墙钟浪费，并隔离 aog 的 OOM 崩溃。

## 2. 思想（为什么有效）
- **独立性**：`solver/mod.rs:16` 的 `aog/rose/edge_csp/pieces/backtrack` 每个都收 `&Puzzle`、返回 `ModuleOutcome`，互不读写对方状态。这是天然的「 embarrassingly parallel 」结构。
- **可中断性**：各模块内部已有周期性 deadline 检查（`aog/search.rs:858`、`backtrack.rs:470`、`edge_csp/mod.rs:458`、`dlx.rs:192`），只要把「超时时钟」换成「共享墙钟 + 取消标志」，它们就能在下一个检查点协作退出。
- **OOM 隔离（真正净增解出的来源）**：aog 自由形状库 OOM（`exit -9`，档① 19 道）当前会**杀掉整个子进程**，导致 rose/edge_csp/pieces/backtrack 全没机会跑 → 题 FAIL。并发若用**独立子进程**，aog OOM 只取消自己，edge_csp 仍能解出（`21` 指出 `0977/0631/1110/0491/1395b` 这 5 道 edge_csp-capable 可被接住）。

## 3. 现状与代码位置
- 串行分发：`rsolver/src/solver/mod.rs:16`（`solve` 函数，依次调用 5 模块，每个传 `timeout_ms`）。
- 每模块 deadline：`mod.rs:53-56` 把 `timeout_ms` 直接灌给各模块（串行时等于每模块都拿满 budget）。
- Python 侧墙钟：`rust_solver.py:147,169` 的 `RUST_PARTS=4`（见 [13-meta-rustparts.md](13-meta-rustparts.md)）。

## 4. 收益（量化估计）
- **净增解出**：档① OOM 题中 edge_csp-capable 的 ~5 道（直接）。其余 14 道优雅超时（仍 FAIL 但不再 `exit -9` 崩溃）。
- **墙钟效率**：单题墙钟从「最坏 4~5×timeout」降到「最快模块的实际耗时」。批量基准（2488 题）吞吐近似 ×4。
- **UX**：UI `solver_runner` 实时求解不再被一个必败模块拖满 timeout。
- **注意**：实测校准（24 §4.1 附录）显示 PASS 题中仅 ~1.3% 由后续模块解出（aog 先赢占 439/445），故并发**不会**提升「超时类」题解出率（每模块本就跑满各自 timeout 串行地），价值在 OOM 隔离 + 墙钟，而非"让超时题变可解"。

## 5. 代价与风险
- **风险：低**（若用子进程隔离则几乎为零；用线程需保证撤销栈线程局部，见 [02](02-parallel-intra.md)）。
- **代价**：新增一个「共享时钟 + 取消原语 + first-wins barrier」约 150–300 行；子进程方案略多（进程管理 + IPC 传解）。
- **正确性**：不受影响——最终解仍过 `validate::validate` 独立验证门（`mod.rs:291`）。

## 6. 优先级 / ROI
- **P1**，ROI 高（零算法风险、当天到数日可出、直接接住 OOM 题）。是 24 §8 速赢 S1。

## 7. 实现思路
**方案 A（线程，简单）**：
```
// solver/race.rs
pub fn solve_race(puzzle: &Puzzle, timeout_ms: u64) -> RaceOutcome {
    let solved = Arc::new(Mutex::new(None));      // Option<Solution>
    let start = Instant::now();
    let mut handles = vec![];
    for m in [aog, rose, edge_csp, pieces, backtrack] {
        if !m.capable(puzzle) { continue; }
        let solved = solved.clone(); let start = start.clone();
        handles.push(thread::spawn(move || {
            let dl = || start.elapsed().as_millis() as u64 > timeout_ms
                  || solved.lock().unwrap().is_some();
            // 各模块 deadline 检查点需接受可调用闭包 dl()
            match m.solve_with_cancel(puzzle, &dl) {
                Solved(sol) => { *solved.lock().unwrap() = Some(sol); }
                _ => {}
            }
        }));
    }
    for h in handles { h.join(); }
    solved.lock().unwrap().take().map(Outcome::Solved).unwrap_or(Outcome::None)
}
```
需把各模块 `solve(puzzle, timeout_ms)` 改为 `solve_with_cancel(puzzle, &dyn Fn()->bool)`（在现有 deadline 检查点插入 `|| cancel()`）。
**方案 B（子进程，稳）**：每模块一个 `rsolver --only=<module>` 子进程，`select` 等先返回者；aog OOM 不影响其他。IPC 用临时文件 / 管道传解。更稳但进程管理更重。

## 8. 验证方法（防回归）
- 用现有 `benchmark_rust_solver.py --baseline` 对比串行 vs 并发：要求 **REGRESSION=0**、**NEW≥5**（OOM 接住）、墙钟均值下降。
- 并发后确定性下降：固定 seed（`14-meta-determinism.md`）并对 borderline 题 `--retry-timeouts` 单独确认。
- 单元：构造一道 aog-OOM 题，断言并发模式返回 edge_csp 解而非 FAIL。

## 9. 依赖与前置
- 前置：[13-meta-rustparts.md](13-meta-rustparts.md)（并发后 `RUST_PARTS` 应改 1，墙钟读数才真实）。
- 依赖各模块 deadline 检查点已可注入取消闭包（当前为全局时钟，需小幅重构）。

## 10. 参考
- `docs/优化/24` §4.1、`§8 S1`；`21` 档① OOM 根因；`11` §4.5C（谜题内并行少见，本文补模块间）。
