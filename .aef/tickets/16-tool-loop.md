# 16: 工具调用循环与协议

**里程碑**：M2
**被阻塞于**：无
**涉及模块**：runtime、agent、conversation

## 要做什么
agent 把工具声明交给模型，模型发出工具调用时经 RPC `tool.call` 交给 runtime 执行，结果回填后继续，直到模型给出最终回复。

## 验收条件
- [ ] 协议：`context.build` 返回工具声明；agent→server 请求 `tool.call {runId, name, arguments}`；`run.event` 增加 tool_start / tool_end
- [ ] 一次运行内的每条 assistant/toolResult 消息都存 conv_message（role 扩展 tool），回放时原样给模型
- [ ] 单次运行最多 50 步工具调用，超出报错结束；中止时正在执行的工具也取消
- [ ] 测试：faux 模型发出工具调用 → 假工具 → 最终回复；消息顺序正确
