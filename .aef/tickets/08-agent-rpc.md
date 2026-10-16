# 08: agent 子进程与 RPC 协议

**里程碑**：M1
**被阻塞于**：06、07
**涉及模块**：runtime、agent
**数据变更**：无

## 要做什么
server 启动时拉起 Node agent 子进程，经 stdio NDJSON JSON-RPC 2.0 双向通信；agent 崩溃自动重启。

## 验收条件
- [x] 握手 `initialize` 带协议版本，版本不符 server 拒绝并报错
- [x] server→agent：`run.start`、`run.cancel`；agent→server 通知 `run.event`；agent→server 请求 `context.build`
- [x] agent 用 pi-ai 流式调用模型，事件映射为 text_delta / thinking_delta / done / error
- [x] 协议类型由 Rust ts-rs 生成到 agent/src/protocol/generated
- [x] agent 测试用 pi-ai faux provider，不连网
- [x] Rust 测试：用假 agent 脚本验证握手、事件流、崩溃重启
