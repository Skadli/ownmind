# 01: 脚手架与本地一键启动

**里程碑**：M1（phase-02 行走骨架）
**被阻塞于**：无
**涉及模块**：全部（空壳）、server
**数据变更**：无

## 要做什么
新机器 clone 仓库后，装好 Rust 与 Node，一条命令把 server 和管理页面跑起来，浏览器能打开页面，页面能拿到 server 的健康状态。

## 验收条件
- [x] Cargo workspace 含 crates/* 全部模块 crate，每个 Cargo.toml 只依赖 module.yaml 的 depends_on
- [x] 非组装 crate 的 lib.rs 只有 `pub mod api; mod internal;`
- [x] server 提供 GET /healthz → 200 `{"status":"ok"}`（集成测试覆盖）
- [x] server 托管 web 构建产物；未知路径回落到 index.html（集成测试覆盖）
- [x] pnpm workspace 含 @ownmind/agent、@ownmind/web
- [x] `pnpm install && pnpm dev` 同时启动 server 与 web 开发服务器，web 经代理显示健康状态
- [x] `cargo test --workspace`、`cargo clippy -- -D warnings`、`cargo fmt --check`、`pnpm -r test`、`pnpm biome check .` 全绿

## 备注
命令以 docs/stack.md 为准；crate 包名 ownmind-<模块>。
