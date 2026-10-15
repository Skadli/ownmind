# 002 Rust 主进程与 TS agent 通过 stdio JSON-RPC 通信

- 状态：已采纳
- 日期：2026-10-15

## 问题
agent 用 TS（pi），其余用 Rust，需要确定两者如何协作、谁管谁的生命周期、数据由谁持有。

## 决定
Rust 是主进程，负责登录、数据库、定时任务、沙盒管理；agent 是 Rust 拉起的 Node 子进程，双方通过 stdio 上的 JSON-RPC 2.0 双向通信。Rust 发起运行请求，agent 推送事件流；记忆、定时任务等工具由 agent 反向调用 Rust 完成，数据只由 Rust 持有。

## 考虑过的方案（及放弃原因）
- agent 独立 HTTP/WebSocket 服务：要开端口、多一份鉴权、生命周期分离。
- agent 直接读写 SQLite：两个进程写同一库、数据失去唯一主人。
- 消息队列：单机单人场景过重。

## 后果
需要定义并版本化 RPC 协议（方法、事件）；agent 崩溃由 Rust 负责重启；agent 本身无状态，可随时重启。
