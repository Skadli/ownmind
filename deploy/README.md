# 部署

前提：一台装了 Docker（含 compose 插件）的 Linux 服务器，一个指向它的域名，80/443 端口开放。

一键安装（把域名换成你的）：

```bash
curl -fsSL https://raw.githubusercontent.com/Skadli/ownmind/main/deploy/install.sh | bash -s -- ownmind.example.com
```

或手动：

```bash
git clone https://github.com/Skadli/ownmind.git
cd ownmind/deploy
cp .env.example .env    # 改 OWNMIND_DOMAIN
docker compose up -d
docker compose ps       # migrate 为 exited (0)，ownmind 为 healthy
```

浏览器打开 `https://你的域名`，首次进入设置主人密码。

- 升级：`docker compose pull && docker compose up -d`（每次启动都会先跑迁移）
- 数据：卷 `ownmind_ownmind-data`（SQLite），备份前先 `docker compose stop ownmind`
- 日志：`docker compose logs -f ownmind`
- 从源码构建镜像：在仓库根目录 `docker build -t ownmind:local .` 和 `docker build --target sandbox -t ownmind-sandbox:local .`，`.env` 里 `OWNMIND_IMAGE=ownmind:local`、`OWNMIND_SANDBOX_IMAGE=ownmind-sandbox:local`
- 沙盒：`sandbox` 服务是 agent 执行命令、读写文件的地方，只挂自己的工作目录卷（`sandbox-workspace`），不暴露端口、不挂 docker.sock；CPU/内存上限在 `.env` 里调。想清空沙盒：`docker compose down && docker volume rm ownmind_sandbox-workspace`
