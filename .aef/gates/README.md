# 门禁

每个门禁一个目录：`run`（通过返回 0）、`fixtures/bad/`（至少一个违规样例）、`README.md`（规则、理由、失效条件）。
`./run-all.sh <repo>` 先确认每个门禁会拦住 bad 样例，再检查仓库。`lib_aef.py` 是共用的 module.yaml 读取。

| 门禁 | 规则 |
|---|---|
| module-deps | crate 依赖 ⊆ depends_on，无环，lib.rs 只公开 api |
| migration-lock | 已登记迁移不可改/删，新迁移必须登记 |
| table-owner | 模块 SQL 只引用自己 owns_tables 的表 |

lint/格式/类型、测试（含空库迁移测试）由 CI 直接运行命令，见 .github/workflows/ci.yml。
