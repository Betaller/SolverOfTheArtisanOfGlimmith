# results/ 目录说明

本目录统一归档每次求解优化的结果。**禁止在 `results/` 根目录散放文件**，必须放入以下三个子目录：

| 目录 | 内容 | 命名规则 | 示例 |
|---|---|---|---|
| `bin/` | rsolver 可执行文件（release 构建） | `rsolver-<commit-id>-<platform>`（短 sha，`-` 分隔） | `rsolver-f1cfa16-linux-x86_64` |
| `bench/` | `scripts/benchmark_rust_solver.py` 的测试结果 | `<日期>_<commit-id>_<short-message>.txt`（日期 `YYYYMMDD`） | `20260807_c6cb307_opt-v3-bench.txt` |
| `tmp/` | 临时测试结果（verify 扫描、根因分析、内存/性能对比等） | 建议 `<日期>_<commit-id>_<short-message>.txt` | `20260806_82c9132_verify-full.txt` |

## 每次优化必须保留两件事

影响求解结果（可解性 / 性能 / 规则语义）的优化提交，必须同时保留：

1. **可执行文件**：产出该结果的 rsolver 二进制 → `bin/rsolver-<commit-id>-<platform>`，随提交入库。
2. **bench 结果**：`scripts/benchmark_rust_solver.py` 的测试输出 → `bench/<日期>_<commit-id>_<short-message>.txt`，随提交入库。

其余临时验证 / 分析 / 对比输出 → `tmp/`，不入库；确认有价值再升格到 `bench/` 或文档。

## 命名约定

- `commit-id`：产生该结果的提交短 sha（7 位）。
- `platform`：如 `linux-x86_64`、`windows-x86_64`。
- 归档文件内注明来源脚本与命令（benchmark 脚本会自动打印 header）。

## 豁免

纯文档、无行为变化的重构等不影响求解结果的提交，可豁免二进制 / bench 归档要求。

## 归档命令参考

```powershell
# 1. 构建并复制二进制
cd rsolver && cargo build --release
Copy-Item rsolver/target/release/rsolver* ..\results\bin\rsolver-<commit-id>-<platform>

# 2. 跑 bench 并保存输出
python scripts/benchmark_rust_solver.py --dir puzzles/official --timeout 40 -j 8 | Tee-Object ..\results\bench\<日期>_<commit-id>_<short-message>.txt
```
