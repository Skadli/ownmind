# 06: 模型供应商凭据与默认模型

**里程碑**：M1
**被阻塞于**：无
**涉及模块**：settings、server、web
**数据变更**：新表 settings_credential、settings_preference

## 要做什么
主人在设置 → 账户里添加模型供应商（7 种：OpenAI 兼容、Anthropic 兼容、OpenAI、Anthropic、Google、DeepSeek、OpenRouter），填 API Key（兼容类再填地址），并选默认模型。Key 加密存储，页面只显示末 4 位。

## 验收条件
- [x] 首次启动在数据目录生成 master.key（32 字节，权限 600），之后复用
- [x] Key 用 XChaCha20-Poly1305 + 主密钥加密入库；数据库里查不到明文
- [x] API：列出/新增/修改/删除凭据，设置/读取默认模型；未登录 401
- [x] 兼容类必须有 http(s) 地址；内置类不能填地址以外的未知供应商
- [x] 删除默认模型所用的凭据时默认模型一起清空
- [x] 账户页可增删改凭据、选默认模型（中英文）

## 备注
决策 008。runtime 通过 settings api 取解密后的 Key，Key 不回传给浏览器。
