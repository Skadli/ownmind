# 技术栈与落地方式

所有技术细节只写在这里，其他文档引用本文件。改栈先写决策记录（docs/decisions/004-tech-stack.md）。

## 选型
| 类别 | 选择 | 版本（2026-10） | 理由 |
|---|---|---|---|
| 后端语言 | Rust（edition 2024） | 1.99 | 主人指定，非 agent 部分全用 Rust |
| Web 框架 | Axum + tokio | 0.8 / 1.53 | tokio 官方，最主流，SSE/WebSocket 现成 |
| 数据库 | SQLite（WAL） | — | 单人单机，零运维，备份即拷文件 |
| 数据访问/迁移 | sqlx（编译期检查 SQL，自带迁移） | 0.9 | 换 Postgres 只改驱动 |
| agent | TypeScript + pi（@earendil-works/pi-agent-core、pi-ai） | 1.2 | 主人指定；多供应商统一 API |
| JS 运行时 | Node.js | 22 LTS | pi 要求 ≥20 |
| 前端 | React + TypeScript + Tailwind + shadcn/ui | 19 / 7 / 4 | AI 最熟；与 agent 同语言 |
| 前端编译 | Rsbuild（Rspack） | 2.x | 主人不用 Vite；Rust 实现，shadcn 支持 |
| 包管理 | Cargo workspace；pnpm workspace（agent、web） | pnpm 12 | 单仓 |
| Rust↔TS 类型 | ts-rs 从 Rust 类型生成 TS 类型 | — | 契约只有一个来源 |
| 测试 | cargo test；vitest；Playwright（端到端） | vitest 5 / PW 1.64 | 集成测试用临时 SQLite 文件，无需容器 |
| Lint/格式 | clippy + rustfmt；Biome | Biome 2.5 | 都是 Rust 实现，快 |
| 部署 | Docker Compose + 一键安装脚本；Caddy 自动 HTTPS | — | 2C4G Linux 云服务器 |

## 命名
- 项目名 Ownmind；Rust crate 名 `ownmind-<模块>`（server 二进制 `ownmind`，沙盒 `ownmind-sandboxd`）；npm 包 `@ownmind/agent`、`@ownmind/web`。
- 环境变量前缀 `OWNMIND_`；镜像 `ghcr.io/skaduv/ownmind`、`ghcr.io/skaduv/ownmind-sandbox`。

## 进程与容器
- `server`（Rust 主进程）：HTTP、登录、数据库、定时任务；托管 web 静态文件；拉起 agent 子进程（stdio JSON-RPC，决策 002）。
- `agent`（Node 子进程，与 server 同容器）：无状态，工具需要的数据一律反向 RPC 调 server。
- `sandbox` 容器：内含 `sandboxd`（Rust），server 经内部网络 + 共享令牌调用；限 CPU/内存，只挂工作目录卷；不挂 docker.sock（决策 003）。
- `caddy`：反向代理 + 自动证书。`searxng`：可选，Compose profile `search` 开启。

## 目录
```
Cargo.toml                 # workspace
crates/
  kernel/                  # 共享内核：ID、时间、错误类型（保持极小）
  <module>/                # 一个业务/平台模块一个 crate
    module.yaml
    Cargo.toml             # [dependencies] 只能出现 module.yaml depends_on 里的模块
    src/lib.rs             # 只 pub 出 api
    src/api.rs | src/api/  # 公共入口：服务 trait/函数、DTO、事件
    src/internal/          # 实现，不 pub
    migrations/            # 本模块的表
    tests/                 # 只通过 api 测试
  server/                  # 组装根：main、路由、配置、启动迁移（不放业务逻辑）
  sandboxd/                # 沙盒执行服务二进制
agent/                     # TS agent（pnpm 包）
  src/tools/ src/skills/ src/rpc/ src/protocol/generated/
  skills/builtin/          # 预设技能（只读，随版本发布）
web/                       # 管理页面（pnpm 包）
  src/pages/ src/components/ui/ src/api/generated/
deploy/                    # compose.yaml、Caddyfile、install.sh
.aef/ docs/
```
运行时数据目录（卷）：`/data/app.db`、`/data/skills/installed/`（已装技能，升级不覆盖）、沙盒 `/workspace`。

## 命令
| 用途 | 命令 |
|---|---|
| 安装依赖 | `pnpm install` |
| 建库/迁移 | `cargo run -p ownmind-server -- migrate`（空库一条命令建好；server 启动时也会执行） |
| 开发运行 | `pnpm dev`（同时起 server 与 web，web 在 :5173，server 在 :8787） |
| 测试 | `cargo test --workspace && pnpm -r test` |
| 端到端 | `pnpm -C web e2e` |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check && pnpm biome check .` |
| 生成 TS 类型 | `cargo test -p ownmind-server export_bindings`（ts-rs） |
| 门禁 | `.aef/gates/run-all.sh .` |
| 本地完整部署 | `docker compose -f deploy/compose.yaml up -d` |

## 通用规则在本栈的落地

### 模块边界
crate 依赖即边界：`crates/<m>/Cargo.toml` 只能依赖 module.yaml `depends_on` 中的模块和 kernel。门禁 `module-deps` 读 `cargo metadata` 对照 module.yaml。lib.rs 只导出 api：
```rust
// crates/conversation/src/lib.rs
pub mod api;
mod internal;
```

### 迁移
- 位置 `crates/<m>/migrations/`，文件名 `<YYYYMMDDHHMMSS>_<m>_<desc>.sql`（版本全局唯一）。
- 各模块一个 `sqlx::migrate!()`，server 按依赖顺序执行，共用 `_sqlx_migrations` 表，`set_ignore_missing(true)`。
- 已合并迁移不可改：门禁 `migration-lock` 对比 `migrations.lock` 中的哈希。
```rust
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
```

### 种子数据
- 参考数据（预设供应商列表等）：迁移里 `INSERT ... ON CONFLICT DO NOTHING`，幂等。
- 初始化数据：首个主人在首次打开管理页面时自己设密码，仓库内无默认账号。
- 演示数据：`cargo run -p ownmind-server -- seed-demo`，只在开发用。

### 错误与事务
- 每个模块一个 `thiserror` 错误枚举；server 统一转 HTTP 状态码。不吞错误，`?` 向上抛。
- 事务在模块 api 内部开、内部提交，不跨模块。
```rust
let mut tx = self.db.begin().await?;
sqlx::query!("INSERT INTO conv_message (id, conversation_id, body) VALUES (?, ?, ?)", id, cid, body)
    .execute(&mut *tx).await?;
tx.commit().await?;
```

### 输入校验
只在系统边界校验（HTTP handler、RPC 入口、sandboxd 入口），用 serde 反序列化 + 显式检查；模块内部信任类型。

### 配置与密钥
- 环境变量 + `/data` 下运行时设置；server 启动时读一次进 `Config` 结构体。
- API Key 等密钥存数据库前用部署时生成的主密钥加密（主密钥在 `/data/master.key`，不进仓库）。

### RPC 与类型
- Rust 类型加 `#[derive(TS)]`，生成到 `agent/src/protocol/generated/`、`web/src/api/generated/`，生成文件不手改。
- server↔agent：JSON-RPC 2.0，每行一条消息（NDJSON），协议版本号写在握手里。
