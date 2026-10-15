# .aef/state.md

## 当前
- 阶段：phase-01（aef-stack 定栈中）
- 里程碑：-

## 里程碑
| 里程碑 | 目标（一句话） | 状态 |
|---|---|---|
| M1 | | 未开始 |

## 本迭代票
- [ ] NN 标题

## 下一步
启动语：读 AGENTS.md 和 .aef/state.md，执行 phase-01。

## 未决问题
- 已定栈：Rust 后端 Axum（tokio）
- 已定栈：SQLite（WAL）+ sqlx（含 sqlx 迁移）
- 已定栈：前端 React + TypeScript + Tailwind + shadcn/ui，静态文件由 Axum 托管；不用 Vite（主人要求）
- 已定栈：前端编译 Rsbuild（Rspack）
- 已定栈：单仓；crates/（Cargo workspace，一模块一 crate）、agent/（TS）、web/（React），agent+web 用 pnpm workspace
- 已定：Rust 主进程 + agent Node 子进程，stdio JSON-RPC（决策 002）
- 已定：沙盒为独立常驻容器 + 容器内 Rust 执行服务，不挂 docker.sock（决策 003）
- 联网搜索后端（追问中）
- 测试工具、门禁工具（待问）
- Rust 与 TS agent 模块如何通信（phase-01）
