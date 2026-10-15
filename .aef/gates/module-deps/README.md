# module-deps
**规则**：crates/<m>/Cargo.toml 中的 ownmind-* 依赖必须列在该模块 module.yaml 的 depends_on；模块依赖无环；业务/平台模块 lib.rs 只有 `pub mod api;` 一条公开声明。
**理由**：AGENTS 硬规则 1——只通过公共入口访问其他模块，内部才能自由重构。
**失效条件**：模块不再一模块一 crate，或 module.yaml 改用块状列表（脚本只解析 `[a, b]` 形式）。
