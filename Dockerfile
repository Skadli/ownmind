# syntax=docker/dockerfile:1.7
# Ownmind 镜像：默认目标是主服务（Rust server + 管理页面静态文件 + Node，决策 002）；
# `--target sandbox` 产出沙盒镜像（sandboxd + 常用工具，决策 003）。

FROM node:22-bookworm-slim AS web
WORKDIR /src
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY web/package.json web/
COPY agent/package.json agent/
RUN --mount=type=cache,target=/root/.local/share/pnpm/store \
    pnpm install --frozen-lockfile
COPY web web
COPY agent agent
RUN pnpm -C web build \
 && pnpm -C agent build \
 && pnpm --filter @ownmind/agent deploy --prod --legacy /out/agent \
 && cp -r agent/dist /out/agent/dist

FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p ownmind-server -p ownmind-sandboxd \
 && cp target/release/ownmind target/release/ownmind-sandboxd /usr/local/bin/

# 沙盒：agent 的命令在这里跑。只挂工作目录卷和只读令牌卷，不挂 docker.sock。
FROM debian:bookworm-slim AS sandbox
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      ca-certificates tini curl wget git python3 python3-pip python3-venv \
      jq less procps unzip xz-utils file \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --uid 10001 --create-home --home-dir /home/sandbox sandbox \
 && mkdir -p /workspace && chown sandbox /workspace
COPY --from=server /usr/local/bin/ownmind-sandboxd /usr/local/bin/ownmind-sandboxd
# 预设技能：随镜像只读同步，升级镜像即更新；已装技能在 /workspace/.skills 不受影响。
COPY agent/skills/builtin /opt/ownmind/skills/builtin
ENV OWNMIND_SANDBOX_ADDR=0.0.0.0:8788 \
    OWNMIND_SANDBOX_WORKDIR=/workspace \
    OWNMIND_SANDBOX_TOKEN_FILE=/secret/sandbox.token
USER sandbox
WORKDIR /workspace
VOLUME /workspace
EXPOSE 8788
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 \
  CMD ["curl", "-fsS", "http://127.0.0.1:8788/healthz"]
ENTRYPOINT ["tini", "--", "ownmind-sandboxd"]

FROM node:22-bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tini \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /data ownmind \
 && mkdir -p /data /secret && chown ownmind /data /secret
COPY --from=server /usr/local/bin/ownmind /usr/local/bin/ownmind
COPY --from=web /src/web/dist /app/web
COPY --from=web /out/agent /app/agent
ENV OWNMIND_ADDR=0.0.0.0:8787 \
    OWNMIND_AGENT_CMD="node /app/agent/dist/index.js" \
    OWNMIND_WEB_DIR=/app/web \
    OWNMIND_DATA_DIR=/data \
    OWNMIND_SANDBOX_TOKEN_FILE=/secret/sandbox.token
USER ownmind
VOLUME /data
EXPOSE 8787
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 \
  CMD ["node", "-e", "fetch('http://127.0.0.1:8787/healthz').then(r=>process.exit(r.ok?0:1),()=>process.exit(1))"]
ENTRYPOINT ["tini", "--", "ownmind"]
