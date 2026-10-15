# AGENTS.md

本文件是 AI 的入口，保持在 100 行以内。

## 项目
开源、自托管的个人 AI agent 平台，个人部署到云服务器后通过浏览器管理页面（后续 App）使用。范围见 docs/SCOPE.md，术语见 docs/GLOSSARY.md，架构见 docs/architecture.md，技术栈与具体做法见 docs/stack.md。

## 工作方式
- 这个项目做什么只以 docs/SCOPE.md 为准。docs/SCOPE.md 为空时，问用户想做什么，不要推测或推荐产品类型。技能里出现的表名、模块名、业务名词都只是举例。
- 会话开始先读本文件和 .aef/state.md，按其中指定的阶段（.aef/phases/）工作。
- 一个会话做一个单元（一张票）；结束前更新 .aef/state.md。
- 小改动（一两个文件、无数据和接口变化）直接改 + 测试，不走切票。
- 技术栈没定（docs/stack.md 不存在）时，只遵守下面的通用规则，不自行选型。

## 命令
技术细节见 docs/stack.md。
- 测试：`cargo test --workspace && pnpm -r test`
- Lint：`cargo clippy --workspace -- -D warnings && cargo fmt --check && pnpm biome check .`
- 建库：`cargo run -p server -- migrate`
- 运行：`cargo run -p server` + `pnpm -C web dev`
- 门禁：`.aef/gates/run-all.sh .`

## 目录
- `crates/<模块>/`：Rust 模块（api 公开、internal 私有、migrations、tests、module.yaml）；`crates/server` 组装根；`crates/sandboxd` 沙盒执行服务
- `agent/`：TS agent（pi）；`agent/skills/builtin/` 预设技能
- `web/`：管理页面（React）
- `deploy/`：Compose、Caddyfile、安装脚本

## 硬规则（每条带理由；总数不超过 10 条）
1. 只通过其他模块的公共入口访问它。理由：内部可以自由重构。
2. 一张表只属于一个模块，只有它能写；跨模块不建外键。理由：数据有唯一负责人，模块能独立迁移。
3. 已合并的迁移不可修改；破坏性数据变更走 expand–contract。理由：别的环境已执行过，升级时新旧代码共存。
4. 密钥和默认密码不进仓库。理由：仓库会被复制。
5. 命名使用 docs/GLOSSARY.md 的词。理由：一个概念一个名字。

## 编码偏好
只在系统边界校验输入，模块内部信任类型；错误大声失败，不吞异常；不为不可能的状态兜底，不为假想需求抽象，不留兼容垫片；能删就删。
