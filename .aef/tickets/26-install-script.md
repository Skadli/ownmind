# 票 26：一键安装脚本与发布加固
- [x] deploy/install.sh：检查 docker/compose/curl，下载 compose.yaml、Caddyfile、.env.example，写 .env（不覆盖已有），pull + up -d
- [x] deploy/README.md 加一键安装说明
- [ ] 发布加固（固定版本镜像标签、备份脚本）留到 27 验收前按需补
