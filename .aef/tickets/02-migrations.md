# 02: 迁移体系

**里程碑**：M1（phase-02 行走骨架）
**被阻塞于**：01
**涉及模块**：kernel、auth、conversation、server
**数据变更**：新表 auth_owner、auth_login_session、conv_conversation

## 要做什么
空数据目录上执行一条命令，建好当前全部表；重复执行无副作用；server 启动时也自动迁移。

## 验收条件
- [x] `cargo run -p ownmind-server -- migrate` 在空目录建好 app.db 与全部表，重复执行成功（集成测试）
- [x] 数据库为 WAL、外键开启（集成测试）
- [x] 各模块迁移在自己目录，server 合并后按版本全局排序执行
- [x] kernel 提供 UUIDv7 ID 与 UTC 毫秒时间（决策 007）
- [x] 仓库内无默认账号；首个主人在首次打开管理页面时设密码（票 03 实现）

## 备注
其余表随各自的票加入；迁移哈希锁在票 04 门禁实现。
