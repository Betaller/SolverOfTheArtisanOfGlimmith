# TAGSolver — 《格里米斯的工匠》求解器

《格里米斯的工匠》是一款网格划分+区域填色解谜游戏的求解器与编辑器。
支持 22 种规则约束，提供 PySide6 图形界面用于输入题目和展示求解结果。

## 文档

- [架构设计](docs/architecture.md) — 系统架构、模块设计、数据流
- [开发指南](docs/development.md) — 环境搭建、编码规范、构建发布
- [验收标准](docs/acceptance.md) — 功能/性能/稳定性验收项
- [测试计划](docs/testing.md) — 测试策略、用例、执行方式

## 快速开始

```powershell
python -m venv .venv
.venv\Scripts\Activate.ps1
pip install -r requirements.txt
python src/app.py
```

## 项目状态

当前处于文档阶段，即将进入模型层和求解器核心开发。

## 许可证

MIT
