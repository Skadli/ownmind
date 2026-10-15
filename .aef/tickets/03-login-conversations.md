# 03: 登录 + 会话 CRUD

**里程碑**：M1（phase-02 行走骨架）
**被阻塞于**：02
**涉及模块**：auth、conversation、server、web
**数据变更**：无（用 02 的表）

## 要做什么
主人首次打开管理页面设密码并直接进入；之后用密码登录；登录后能新建、改名、删除会话；退出登录后接口拒绝访问。

## 验收条件
- [x] 首次设置只能一次，密码至少 8 字符，Argon2 哈希（auth 测试 + HTTP 测试）
- [x] 登录发 HttpOnly、Secure、SameSite=Strict Cookie，30 天有效，使用即续期；库里只存令牌 SHA-256（auth 测试）
- [x] 退出登录后令牌失效；未登录访问会话接口 401（HTTP 测试）
- [x] 会话新建/列表（按更新时间倒序）/改名/删除，不存在返回 404，空标题 422（conversation 测试 + HTTP 测试）
- [x] 错误统一为 `{"error": code}`，前端按 code 翻译（中/英）
- [x] Conversation 类型由 ts-rs 生成到 web/src/api/generated
- [x] 前端：设置密码页、登录页、会话列表页；本地真实 server 手动走通

## 备注
TOTP 留到 M1 后续票。登录暂无限速（Argon2 本身拖慢暴力尝试），记入未决。shadcn/ui 组件在需要时再引入，目前纯 Tailwind。
