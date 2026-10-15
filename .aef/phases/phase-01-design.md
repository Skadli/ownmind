# phase-01 设计

## 目标
定技术栈和模块结构，定数据归属和横切能力，排出里程碑。

## 先读
docs/SCOPE.md、docs/GLOSSARY.md、.aef/state.md。

## 用到的技能
aef-stack（先做）、aef-architecture、aef-data

## 产出
- docs/stack.md（技术栈与在本栈中的落地方式；与用户讨论确定后生成）
- modules/<name>/module.yaml × N
- docs/architecture.md：模块图、依赖方向、“新功能放哪”表、横切能力结论、各模块的表
- AGENTS.md 填实命令、目录、硬规则
- .aef/state.md 中的里程碑列表

## 完成标准
用户确认技术栈与里程碑；每个名词有唯一归属模块，每张表有唯一 owner；模块依赖无环。

## 不要做
不写业务代码；不为只有一个实现的能力设计插件接口。
