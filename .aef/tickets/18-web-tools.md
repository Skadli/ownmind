# 18: 联网搜索与抓网页

**里程碑**：M2
**被阻塞于**：16
**涉及模块**：settings、runtime、web

## 验收条件
- [x] 账户页可填 Brave Search 或 Tavily 的 Key（加密存储，同模型 Key）；可选 SearXNG 地址
- [x] web_search 工具：返回标题、链接、摘要；未配置时返回明确提示
- [x] web_fetch 工具：server 端抓取，HTML 转正文文本，限大小与超时；拒绝内网地址（SSRF）
- [x] 测试用本地假服务器，不连外网
