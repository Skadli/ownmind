# 17: 命令与文件工具

**里程碑**：M2
**被阻塞于**：15、16
**涉及模块**：sandbox、runtime、web

## 验收条件
- [x] 工具 exec、read_file、write_file、edit_file 经 sandbox 客户端调用 sandboxd
- [x] 单次工具结果超过 8k token 时截断，全文写入沙盒 /workspace/.outputs/<id>，结果里给路径（决策 005.3）
- [x] 组装上下文时，超过最近 10 轮的工具结果替换为一行占位（决策 005.4）
- [x] 聊天界面显示工具调用（名字、参数摘要、可展开结果）
