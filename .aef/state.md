# .aef/state.md

## 当前
- 阶段：phase-04 单票（M1 第一迭代）
- 里程碑：M2（M1 待主人服务器验证）

## 里程碑
| 里程碑 | 目标（一句话） | 状态 |
|---|---|---|
| M1 能聊 | 部署后登录就能和 agent 对话（Compose、登录/TOTP、账户页 Key、流式聊天、上下文压缩）；新服务器装好连续聊 100 轮不崩 | 进行中 |
| M2 能干活 | 沙盒、8 个工具、搜索、记忆、技能与 Git 安装；一句话让它查资料并写成文件 | 未开始 |
| M3 自己跑（MVP） | 定时任务、其余设置页、一键脚本、发布加固；发布 v0.1.0 | 未开始 |
| M4 能代办 | 浏览器自动化、第三方账号、passkey；开工前再细化 | 未开始 |

## 本迭代票（phase-02）
- [x] 01 脚手架与本地一键启动（.aef/tickets/01-scaffold.md）
- [x] 02 迁移体系（.aef/tickets/02-migrations.md）：auth_*、conv_conversation，UUIDv7 + UTC 毫秒（决策 007）
- [x] 03 登录 + 会话 CRUD（.aef/tickets/03-login-conversations.md）
- [x] 04 门禁与 CI（.aef/tickets/04-gates-ci.md）：module-deps、migration-lock、table-owner + .github/workflows/ci.yml；CI 已在 GitHub 跑绿
- [~] 05 部署（.aef/tickets/05-deploy.md）：镜像 + Compose + Caddy，CI 冒烟通过，ghcr edge 已发布；只差主人服务器验证

## 本迭代票（M1 第一迭代，2026-10-15 切票；主人授权 AI 自行决策）
- [x] 06 供应商凭据与默认模型
- [x] 07 消息存储
- [x] 08 agent 子进程与 RPC 协议（阻塞于 06、07）
- [x] 09 流式聊天（阻塞于 08）
- [x] 10 上下文组装与压缩（阻塞于 09）
- [x] 11 TOTP 两步验证
- [x] 12 登录限速
- [x] 13 设置页骨架（阻塞于 06、11）
- [~] 14 M1 验收：100 轮 faux 聊天集成测试（触发压缩）、CI 部署冒烟加一轮聊天已完成；只差主人在自己服务器用真实 Key 聊通

## 本迭代票（M2 第一迭代，2026-10-17 切票；主人授权 AI 自行决策）
- [x] 15 sandboxd 执行服务与沙盒容器
- [x] 16 工具调用循环与协议
- [x] 17 命令与文件工具（阻塞于 15、16）
- [x] 18 联网搜索与抓网页（阻塞于 16）
- [x] 19 长期记忆
- [ ] 20 技能（阻塞于 17）
- [ ] 21 M2 验收（阻塞于 15–20）

## 下一步
M1 代码完成，等主人服务器验证（05、14）。M2：15–19 完成（19 待 CI）；下一张 20（技能），然后 21。开发在 dev，经 dev→main PR（CI + CodeRabbit）合并。

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
- 已定：许可证 AGPL-3.0-only（决策 006）
- 远程仓库：github.com/Skadli/ownmind（public）
- 已定：登录限速（票 12：5 次失败起 1、2、4… 分钟锁定，上限 1 小时）
- 已定：主键 UUIDv7 文本、时间 UTC 毫秒、STRICT 表（决策 007）
