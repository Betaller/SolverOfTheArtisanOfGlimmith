# 17 · 更快序列化协议

> 状态：🟢 新方向 ｜ 分类：进一步全新方向（N2）｜ 来源：`docs/优化/24` §11.2
> 关联：[03-parallel-puzzle.md](03-parallel-puzzle.md) · [27-corpus-cache.md](27-corpus-cache.md)

## 1. 一句话
子进程协议是 `serde_json` 全量 parse + 每行 JSON；Python 侧 `puzzle_to_dict` 也有开销。改用 `simd-json` / 二进制（flatbuffer）/ 共享内存，消除批量回归的纯 IO 墙钟。

## 2. 思想（为什么有效）
- 2488 题批量时，JSON 编解码本身占可观墙钟：Python `json.dumps`（`rust_solver._prepare_input`）+ Python `json.load` 解析结果 + Rust `serde_json` parse + 序列化。
- 当前 `--batch`（[03](03-parallel-puzzle.md)）已复用**子进程**，但每行仍是 JSON —— 编解码成本未消除。
- 换更快编解码是**纯 IO 收益、零算法风险**：不改搜索语义，正确性由独立验证器兜底。

## 3. 现状与代码位置
- Rust：`rsolver/src/io.rs:325` `parse_puzzle`、`io.rs:346` `solution_to_json_text`（serde_json）。
- Python：`src/solver/rust_solver.py:171` `_prepare_input`（`json.dumps`）、`:182` `_parse_solution`。
- 批量：`main.rs` `--batch` 多行 JSON 逐行进出。

## 4. 收益
- 批量基准（每日 CI 全量回归）墙钟降 10–30%（纯 IO）。
- UI 单次调用延迟下降（尤其大网格 puzzle 序列化）。

## 5. 代价与风险
- **风险：低**（协议兼容性问题可控）。
- **代价**：小–中（Rust 换 `simd-json` 最简；换 flatbuffer/共享内存需改两侧 + 版本协商）。

## 6. 优先级 / ROI
- **P1**，ROI 中（速赢，零算法风险；24 N2）。

## 7. 实现思路
**阶梯 1（最简）**：Rust 侧把 `serde_json` 换成 `simd-json`（API 兼容，需 `&mut [u8]`）。
**阶梯 2**：Python 侧用 `orjson`（比 `json` 快数倍）替代 `json.dumps/loads`。
**阶梯 3（激进）**：二进制帧协议 —— 定义 puzzle/solution 的 flatbuffer schema；或共享内存（`mmap` 传 puzzle，结果走管道）。
**兼容**：保留 `--format=json|bin` 开关，默认 json（向后兼容），bench/CI 用 bin。

## 8. 验证方法
- 等价性：同一题 json / bin 两协议结果完全一致（解析出的 puzzle 一致）。
- `--baseline` REGRESSION=0；对比批量 wall 下降幅度。

## 9. 依赖与前置
- 与 [27-corpus-cache.md](27-corpus-cache.md) 协同（缓存 + 快编解码，批量更快）。

## 10. 参考
- `docs/优化/24` §11.2；`io.rs:325,346`；`rust_solver.py:171,182`。
