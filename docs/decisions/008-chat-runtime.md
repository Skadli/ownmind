# 008 聊天运行与凭据

- 状态：已采纳
- 日期：2026-10-15

## 问题
M1 流式聊天需要定下：Key 怎么存、一次运行怎么在浏览器、server、agent 之间流动、模型怎么解析。

## 决定
1. **凭据**：settings_credential 一行一个供应商连接（种类 + 可选地址 + 加密 Key）；Key 用 XChaCha20-Poly1305 加密，主密钥是数据目录下的 master.key（首次启动生成，权限 600）。Key 只在 server 内解密，经 RPC 传给 agent 当次使用，从不返回浏览器。
2. **默认模型**：settings_preference 的 `default_model` = {credentialId, modelId}。模型 ID 由主人填写；内置供应商在 pi-ai 目录里解析，找不到时按该供应商的 API 与默认地址构造；兼容类一律按地址构造。
3. **运行流程**：浏览器 POST 消息 → server 存主人消息 → runtime 发 `run.start{runId, conversationId, model}` 给 agent → agent 回调 `context.build` 取上下文（决策 005）→ agent 用 pi-ai 流式请求 → `run.event` 通知 → server 转成 SSE 给浏览器，done 时存 assistant 消息。
4. **一会话一运行**：同一会话同时只允许一个运行，浏览器断开不中止运行（结果照样存库），主人可显式中止。
5. **M1 只用 pi-ai**：没有工具，不需要 agent 循环；M2 加工具时换成 pi-agent-core，协议不变（context.build 每轮调用）。

## 考虑过的方案（及放弃原因）
- 浏览器 WebSocket：SSE 足够（单向流 + 普通 POST 中止），过代理更简单。
- Key 明文存库：备份文件一泄露 Key 全丢。
- 用环境变量配 Key：主人要在页面上改，且多个兼容地址。

## 后果
丢失 master.key 等于丢失全部 Key（需重填），备份时要一起备份数据目录。
