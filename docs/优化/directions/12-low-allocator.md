# 12 · 分配器 / 编译选项

> 状态：🟢 新方向 ｜ 分类：底层系统调优 ｜ 来源：`docs/优化/24` §6.4
> 关联：[09-low-hashset.md](09-low-hashset.md) · [27-corpus-cache.md](27-corpus-cache.md)

## 1. 一句话
链接 `mimalloc`/定制分配器 + 开启 `lto`/`codegen-units=1`/`target-cpu=native`：aog 形状库 burst 分配场景比默认分配器稳（避免 swap 抖动致 deadline 不触发，`17`），且 flood-fill 密集循环常获 10–20% 提升。

## 2. 思想（为什么有效）
- **分配器**：aog 形状库突发大量小分配（`shapes.rs` / `core.rs:169`），默认系统分配器在高并发/大形状库下易碎片 + 触发 swap；`mimalloc`/`jemalloc` 的线程本地缓存显著降低延迟与尾部抖动。`17` 指出 swap 抖动会让 deadline 检查「来不及停住」→ 实际超时远超 budget。**换分配器直接治这个盲区**。
- **编译**：`lto=fat` + `codegen-units=1` 跨 crate 内联（求解核心跨 `solver/*` 调用多）；`target-cpu=native` 启用 AVX2 等，flood-fill 位运算/循环向量化受益。

## 3. 现状与代码位置
- `Cargo.toml` 当前 `release` profile（默认 `opt-level=3`，无 `lto`/`codegen-units`）。
- 无 `#[global_allocator]`。
- deadline：`clock.rs` + 各模块检查点（`17` 分析盲区）。

## 4. 收益
- 分配器：消除 swap 抖动 → deadline 真正准时停（直接修正 `17` 的挂死盲区）；大形状库题内存更稳。
- 编译：10–20% 整体加速（无算法改动、零回归风险）。

## 5. 代价与风险
- **风险：低**。`target-cpu=native` 使二进制不可移植（CI 产物需同架构；`results/bin/` 命名含 platform 已规范，`AGENTS.md`）。
- **代价**：极小（`Cargo.toml` + 加一个依赖）。

## 6. 优先级 / ROI
- **P1**，ROI 高（零算法风险、当天可出；24 §8 速赢 S6）。

## 7. 实现思路
```
// Cargo.toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
# 构建：RUSTFLAGS="-C target-cpu=native"

// main.rs 或 lib.rs
#[global_allocator]
static A: mimalloc::MiMalloc = mimalloc::MiMalloc;
```
- CI：区分「native 发布二进制」（进 `results/bin/`）与「通用测试二进制」。

## 8. 验证方法
- `cargo bench` 风格微基准（若无可直接比全量 wall 均值）。
- `--baseline` REGRESSION=0 + wall 下降。
- 观察 aog 大形状库题是否仍 swap（RSS 监控）。

## 9. 依赖与前置
- 无强依赖；与 [09-low-hashset.md](09-low-hashset.md) 协同降压。

## 10. 参考
- `docs/优化/24` §6.4；`17-挂死根因与deadline盲区`；`AGENTS.md`（results/bin 平台命名）。
