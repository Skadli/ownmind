# 15: sandboxd 执行服务与沙盒容器

**里程碑**：M2
**被阻塞于**：无
**涉及模块**：sandboxd、deploy
**数据变更**：无

## 要做什么
沙盒容器里常驻一个 Rust HTTP 服务，只在 Compose 内部网络可达，用共享令牌鉴权（决策 003）。

## 验收条件
- [ ] POST /exec：在工作目录里跑 shell 命令，超时（默认 60 秒、上限 10 分钟）杀进程组，stdout/stderr 各限 1 MB，返回退出码
- [ ] POST /read、/write、/edit（唯一字符串替换）：路径限制在工作目录内（拒绝 .. 与符号链接逃逸）
- [ ] 所有请求要 Bearer 令牌；令牌由主服务首次启动生成、经共享卷传给沙盒
- [ ] Dockerfile 多阶段产出沙盒镜像（含 git、python3、curl 等常用工具），Compose 加 sandbox 服务（限 CPU/内存，只挂工作目录卷，无 docker.sock）
- [ ] 测试覆盖：超时、输出截断、路径逃逸、鉴权失败
