# 21: M2 验收

**里程碑**：M2
**被阻塞于**：15–20

## 验收条件
- [x] 集成测试（crates/server/tests/m2_acceptance.rs，工具链串联）：测试代码按序调用 web_search → web_fetch → write_file 三个工具，沙盒里出现文件（工具循环本身由票 16 的运行时测试覆盖；真实模型驱动由主人真实 Key 验证）
- [x] CI 部署冒烟（已含沙盒 exec）：沙盒容器启动，exec 工具跑通一次
- [ ] 主人用真实 Key 一句话让它查资料并写成文件
