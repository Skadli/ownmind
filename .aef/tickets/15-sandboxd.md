# 15: sandboxd 执行服务与沙盒容器

**里程碑**：M2
**被阻塞于**：无
**涉及模块**：sandboxd、deploy
**数据变更**：无

## 要做什么
沙盒容器里常驻一个 Rust HTTP 服务，只在 Compose 内部网络可达，用共享令牌鉴权（决策 003）。

## 验收条件
- [x] POST /exec：在工作目录里跑 shell 命令，超时（默认 60 秒、上限 10 分钟）杀进程组，stdout/stderr 各限 1 MB，返回退出码
- [x] POST /read、/write、/edit（唯一字符串替换）：路径限制在工作目录内（拒绝 .. 与符号链接逃逸）
- [x] 所有请求要 Bearer 令牌；令牌由主服务首次启动生成、经共享卷传给沙盒
- [x] Dockerfile 多阶段产出沙盒镜像（含 git、python3、curl 等常用工具），Compose 加 sandbox 服务（限 CPU/内存，只挂工作目录卷，无 docker.sock）
- [x] 测试覆盖：超时、输出截断、路径逃逸、鉴权失败

## 实现记录
- crates/sandboxd：lib（router、exec、fs）+ 二进制；`sh -c` + process_group，超时 killpg(SIGKILL)，命令结束后 2 秒仍握管道的后台进程也杀掉。
- 令牌：kernel::load_or_create_token；主服务启动时在 OWNMIND_SANDBOX_TOKEN_FILE（镜像默认 /secret/sandbox.token）生成，`sandbox-secret` 卷只读挂给沙盒；sandboxd 等到令牌出现才监听。
- 镜像：Dockerfile `--target sandbox` → ghcr.io/skadli/ownmind-sandbox；Compose `sandbox` 服务限 CPU/内存/pids，只挂 sandbox-workspace 与只读令牌卷。
- CI 冒烟：从主服务容器经内部网络调 /exec（有令牌 200、无令牌 401），沙盒无 /data。
- 主服务侧客户端（sandbox 模块）留给票 17。
