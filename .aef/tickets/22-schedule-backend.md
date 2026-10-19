# 22: 定时任务后端

**里程碑**：M3
**被阻塞于**：16
**涉及模块**：schedule、server
**数据变更**：新表 sched_task、sched_fire

## 验收条件
- [x] 任务支持固定间隔重复与一次性两种计划；间隔不短于 60 秒；错过的周期不补跑
- [x] 到期任务只被认领一次，发出 TaskDue 事件（Scheduler::subscribe）
- [x] server 调度器每 15 秒检查；到期任务新开会话「定时：名称」并把 prompt 发给 agent，记录触发结果
- [x] CRUD API：/api/schedule/tasks（GET/POST）、/{id}（GET/PATCH/DELETE）、/{id}/fires；需登录
- [x] 测试：schedule 单测 6 个 + server http 集成测试

## 不在本票
- 前端 UI（票 25）；cron 表达式与按日/周计划（后续）
