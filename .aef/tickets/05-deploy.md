# 05: 部署

**里程碑**：M1（phase-02 行走骨架）
**被阻塞于**：04
**涉及模块**：server（SIGTERM）
**数据变更**：无

## 要做什么
主人在自己的服务器上 `docker compose up -d` 就能用 HTTPS 打开 Ownmind。

## 验收条件
- [x] Dockerfile 多阶段：web 构建 + Rust release + node:22-slim 运行时；非 root、tini、HEALTHCHECK /healthz
- [x] deploy/compose.yaml：migrate 一次性服务成功退出后才启动 ownmind；caddy 等 ownmind healthy
- [x] Caddy 用 .env 的 OWNMIND_DOMAIN 自动 HTTPS
- [x] server 响应 SIGTERM 优雅退出
- [x] CI docker job：真实 Compose 走通（HTTPS /healthz、页面、设置密码、建会话、down/up 后数据仍在）
- [x] main 推送后发布 ghcr.io/skadli/ownmind:edge（匿名可拉取）
- [ ] 主人在自己的服务器 + 真实域名上按 deploy/README.md 部署成功

## 备注
沙盒无 docker，镜像与 Compose 验证在 CI 完成（run 38736040293）。
