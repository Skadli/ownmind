# 09: 流式聊天

**里程碑**：M1
**被阻塞于**：08
**涉及模块**：runtime、conversation、server、web
**数据变更**：无

## 要做什么
主人在会话里发消息，agent 的回复逐字流出来；刷新页面历史还在。

## 验收条件
- [x] POST /api/conversations/{id}/messages 存主人消息并返回 SSE 事件流（delta、done、error）
- [x] 回复完成后存 assistant 消息（含 usage）
- [x] 未配置默认模型时返回明确错误，页面引导去账户页
- [x] 同一会话同时只允许一个运行；可中止
- [x] 聊天界面：消息列表、输入框、流式显示、停止按钮、Markdown 文本
