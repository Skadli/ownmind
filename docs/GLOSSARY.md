# GLOSSARY

<!-- 格式：- **术语**（代码中的名字）：定义。禁用：同义词。 -->
- **部署**（deployment）：用户在自己云服务器上运行的一整套本项目实例。禁用：实例、站点。
- **管理页面**（console）：浏览器访问的 Web 界面，用来与 agent 对话和管理部署。禁用：后台、控制台、dashboard。
- **App**（app）：后续开发的客户端，连接某个部署。
- **agent 模块**（agent）：基于 pi 内核、用 TypeScript 实现的智能体运行部分。
- **模型供应商**（provider）：提供大模型 API 的厂商，如 OpenAI、Anthropic。禁用：模型厂商、渠道。
- **工具**（tool）：agent 可调用的单个能力（如网页搜索、读文件）。
- **技能**（skill）：一份可加载的说明与流程，告诉 agent 如何完成某类任务，可组合使用工具。禁用：插件、specialty。
- **主人**（owner）：部署它并唯一使用它的那个人。禁用：用户（泛指时可用）、管理员、租户。
- **会话**（conversation）：主人与 agent 的一条对话线程；一个主人有多个会话。禁用：聊天、session、thread。
- **记忆**（memory）：跨会话保存、agent 自己读写的长期信息。禁用：知识库。
- **定时任务**（scheduled task）：按时间表触发一次 agent 运行。禁用：cron、job。
- **沙盒**（sandbox）：隔离环境，agent 在里面执行命令、读写文件，不影响宿主机。禁用：容器（指代此概念时）。
