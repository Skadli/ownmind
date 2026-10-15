# 门禁

每个门禁一个目录：`run`（通过返回 0）、`fixtures/bad/`（至少一个违规样例）、`README.md`（规则、理由、失效条件）。
`./run-all.sh <repo>` 先确认每个门禁会拦住 bad 样例，再检查仓库。

`_example/` 是一个示例门禁：已合并的迁移不可修改（对比 migrations.lock 中记录的哈希）。
