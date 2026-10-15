# 004 技术栈选择

- 状态：已采纳
- 日期：2026-10-15

## 问题
确定 MVP 的具体技术栈（语言划分见 001、进程通信见 002、沙盒见 003）。

## 决定
见 docs/stack.md 选型表：Axum、SQLite+sqlx、React+TS+Tailwind+shadcn/ui、Rsbuild、单仓 Cargo+pnpm workspace、cargo test/vitest/Playwright、clippy/rustfmt/Biome、Docker Compose+一键脚本。联网搜索默认用主人的 Brave/Tavily Key，SearXNG 可选。

## 考虑过的方案（及放弃原因）
- Actix Web：性能相当，生态向 Axum 靠拢。
- Postgres：单人部署多一个容器、运维和备份成本高。
- Vite：主人不采用，改 Rsbuild。
- esbuild + Tailwind CLI：最轻，但热更新等要自己配。
- 多仓：跨前后端和 agent 的改动要多处提交，门禁分散。
- SearXNG 默认启用：多占内存，改为可选。

## 后果
两种语言工具链并存；契约以 Rust 类型为源，用 ts-rs 生成 TS 类型。
