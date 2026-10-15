//! 公共入口：其他模块只能用这里导出的东西。

/// 本模块的数据库迁移，由 server 按依赖顺序执行。
pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
