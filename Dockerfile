# syntax=docker/dockerfile:1.7
# Ownmind 单镜像：Rust server + 管理页面静态文件 + Node（agent 子进程用，决策 002）。

FROM node:22-bookworm-slim AS web
WORKDIR /src
RUN corepack enable
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY web/package.json web/
COPY agent/package.json agent/
RUN --mount=type=cache,target=/root/.local/share/pnpm/store \
    pnpm install --frozen-lockfile --filter @ownmind/web...
COPY web web
RUN pnpm -C web build

FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates crates
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p ownmind-server \
 && cp target/release/ownmind-server /usr/local/bin/ownmind-server

FROM node:22-bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates tini \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --home /data ownmind \
 && mkdir -p /data && chown ownmind /data
COPY --from=server /usr/local/bin/ownmind-server /usr/local/bin/ownmind-server
COPY --from=web /src/web/dist /app/web
ENV OWNMIND_ADDR=0.0.0.0:8787 \
    OWNMIND_WEB_DIR=/app/web \
    OWNMIND_DATA_DIR=/data
USER ownmind
VOLUME /data
EXPOSE 8787
HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 \
  CMD ["node", "-e", "fetch('http://127.0.0.1:8787/healthz').then(r=>process.exit(r.ok?0:1),()=>process.exit(1))"]
ENTRYPOINT ["tini", "--", "ownmind-server"]
