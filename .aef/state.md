# .aef/state.md

## 当前
- 阶段：phase-02 行走骨架（拆票待主人确认）
- 里程碑：M1

## 里程碑
| 里程碑 | 目标（一句话） | 状态 |
|---|---|---|
| M1 能聊 | 部署后登录就能和 agent 对话（Compose、登录/TOTP、账户页 Key、流式聊天、上下文压缩）；新服务器装好连续聊 100 轮不崩 | 进行中 |
| M2 能干活 | 沙盒、8 个工具、搜索、记忆、技能与 Git 安装；一句话让它查资料并写成文件 | 未开始 |
| M3 自己跑（MVP） | 定时任务、其余设置页、一键脚本、发布加固；发布 v0.1.0 | 未开始 |
| M4 能代办 | 浏览器自动化、第三方账号、passkey；开工前再细化 | 未开始 |

## 本迭代票
- [ ] NN 标题

## 下一步
启动语：读 AGENTS.md 和 .aef/state.md，执行 phase-02。

## 未决问题
- 已定栈：Rust 后端 Axum（tokio）
- 已定栈：SQLite（WAL）+ sqlx（含 sqlx 迁移）
- 已定栈：前端 React + TypeScript + Tailwind + shadcn/ui，静态文件由 Axum 托管；不用 Vite（主人要求）
- 已定栈：前端编译 Rsbuild（Rspack）
- 已定栈：单仓；crates/（Cargo workspace，一模块一 crate）、agent/（TS）、web/（React），agent+web 用 pnpm workspace
- 已定：Rust 主进程 + agent Node 子进程，stdio JSON-RPC（决策 002）
- 已定：沙盒为独立常驻容器 + 容器内 Rust 执行服务，不挂 docker.sock（决策 003）
- 已定：联网搜索用主人在账户页填的 Brave Search / Tavily Key；SearXNG 作为可选组件（Compose profile 开启），不默认启用
- 已定：测试与门禁工具（见 docs/stack.md）
- 已定：项目名 Ownmind（AI 起名，主人可改）
- 已定：模块划分（docs/architecture.md、crates/*/module.yaml）
- 已定：上下文管理（决策 005）
- 已定：里程碑 M1–M4（phase-01 完成）
