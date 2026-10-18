# 20: 技能

**里程碑**：M2
**被阻塞于**：17
**涉及模块**：skill、runtime、agent
**数据变更**：新表 skill_installed

## 验收条件
- [ ] 预设技能放 agent/skills/builtin，随镜像只读同步进沙盒；已装技能单独目录，升级不覆盖
- [ ] 技能索引（名字 + 一句描述，取自 SKILL.md front matter）进上下文第 3 层，agent 需要时读 SKILL.md
- [ ] 在对话里给 Git 链接即可安装（沙盒内 git clone，校验有 SKILL.md，登记 skill_installed）
- [ ] 至少一个预设技能（如“写研究报告”）
