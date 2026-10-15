# 07: 消息存储

**里程碑**：M1
**被阻塞于**：无
**涉及模块**：conversation、server
**数据变更**：新表 conv_message

## 要做什么
会话里的每条消息（主人、agent）按顺序存进库，打开会话能看到完整历史。

## 验收条件
- [x] conv_message：id、conversation_id、seq（会话内递增）、role（user/assistant）、body（pi 消息 JSON）、text（纯文本，用于显示与估算）、input/output tokens、created_at
- [x] 追加消息会刷新会话 updated_at；删会话连带删消息（同模块内）
- [x] GET /api/conversations/{id}/messages 按 seq 返回
