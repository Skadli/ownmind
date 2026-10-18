# table-owner
**规则**：模块 src/ 和 migrations/ 里的 SQL（INTO/UPDATE/FROM/JOIN/TABLE/ON 之后）只能出现本模块 owns_tables 里的表。
**理由**：AGENTS 硬规则 2——一张表只属于一个模块，其他模块经它的 api 访问。
**失效条件**：SQL 不再以字符串写在 Rust/SQL 文件里（例如改用 ORM 生成），或表名不带模块前缀。

例外：`<自有表>__rebuild` 视为自有表（SQLite 改 CHECK 等约束只能建新表、拷数据、改名）。
