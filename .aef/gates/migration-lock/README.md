# migration-lock
**规则**：migrations.lock 中登记的迁移不能修改或删除；每个迁移文件都必须登记。新迁移写好后运行 `.aef/gates/migration-lock/update`，与迁移同一次提交。
**理由**：AGENTS 硬规则 3——别的部署已执行过的迁移一改就会分叉。
**失效条件**：迁移目录不再是 crates/*/migrations/*.sql。
