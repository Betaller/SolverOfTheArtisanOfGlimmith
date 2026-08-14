# 格里米斯的工匠 · Web 版（Vue 3 + WASM）

浏览器内求解器：`rsolver`（Rust）编译为 WebAssembly，在 Web Worker 中运行；前端为
Vue 3 + TypeScript + Vite + Pinia。**纯静态站点**，由 GitHub Pages 托管。

## 结构

```
web/
├── src/
│   ├── worker/solver.worker.ts   # 加载 wasm、在 worker 中求解
│   ├── worker/solverClient.ts    # 主线程 promise 封装
│   ├── lib/types.ts              # 与 rsolver 对齐的 JSON 协议类型
│   ├── lib/model.ts              # 网格模型 / 边·顶点索引 / 区域着色
│   ├── lib/theme.ts              # 主题色 + 22 条规则常量
│   ├── lib/shapes.ts             # 多联骨牌规范化 / 枚举
│   ├── store/puzzle.ts           # Pinia：官方解优先，求解器兜底
│   └── components/               # GridCanvas（SVG 画板）/ 工具 / 规则 / 属性 / 题库
├── scripts/bundle-puzzles.mjs    # 打包官方题 + 官方解 → public/data/
└── public/data/                  # 生成物（gitignore，不入库）
```

## 求解策略（官方解优先）

- **官方题**：`puzzles/official/<Zone>-answer/` 里有规范解（唯一解），前端直接渲染其
  `regions`，**不调用求解器**。
- **无官方解的题 / 用户自建题**：交给 WASM 求解器（worker 中运行，默认 5s 超时）。

对应逻辑见 `store/puzzle.ts` 的 `displayRegions`（answer → solver 兜底）。

## 本地构建

依赖：`node` ≥ 18、`rustup`（`wasm32-unknown-unknown` 目标）、`wasm-pack`。

```bash
# 一次性准备
rustup target add wasm32-unknown-unknown
cargo install wasm-pack   # 或按 https://rustwasm.github.io/wasm-pack/ 安装

cd web
npm install
npm run data        # 打包官方题库 → public/data/
npm run build       # build:wasm（wasm-pack）→ vite build → dist/
npm run preview     # 本地预览 dist/
```

开发模式（含 wasm 重建 + HMR）：

```bash
cd web && npm run dev
```

> `src/wasm/`（wasm-pack 产物）与 `public/data/`（题库打包产物）均为生成物，已
> gitignore，不入库。CI 会在部署前重新生成。

## 部署到 GitHub Pages

推送到 `main` 分支即触发 `.github/workflows/deploy.yml`，把
`web/dist/` 发布到 GitHub Pages。要点：

- `vite.config.ts` 用 `base: './'`（相对路径），兼容 `https://<user>.github.io/<repo>/` 子路径。
- GitHub Pages 默认以 `application/wasm` 提供 `.wasm`，无需额外配置。

## 交互编辑与已知边界

- **交互编辑已完整实现**：规则（全部 22 条，precise/range/shape_pool 参数）、边
  （预画分割线 + heterogeneous/homogeneous/inequality/difference 约束 + 外边界）、
  顶点（望塔）、单元格（blocked / number / symbol / compass / fence / shape pattern）、
  形状池编辑，均可在画板或左右面板操作（工具 / 规则配置 / 属性面板）。
- **已知边界**：浏览器默认 5s 超时（`rsolver/src/wasm.rs::WEB_TIMEOUT_MS`），官方难题
  可能「超时未解」，属预期（离线基准口径为 30s×3 段）。
