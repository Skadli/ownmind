#!/usr/bin/env bash
# Ownmind 一键安装：curl -fsSL https://raw.githubusercontent.com/Skadli/ownmind/main/deploy/install.sh | bash -s -- 你的域名
# 环境变量：OWNMIND_DIR（安装目录，默认 ~/ownmind）、OWNMIND_REF（取文件的分支或标签，默认 main）
set -euo pipefail

DOMAIN="${1:-${OWNMIND_DOMAIN:-}}"
DIR="${OWNMIND_DIR:-$HOME/ownmind}"
REF="${OWNMIND_REF:-main}"
RAW="https://raw.githubusercontent.com/Skadli/ownmind/${REF}/deploy"

die() { echo "错误：$*" >&2; exit 1; }

[ -n "$DOMAIN" ] || die "用法：install.sh 你的域名（DNS 需已指向本机）"
case "$DOMAIN" in
  *[!A-Za-z0-9.-]* | .* | *. | "") die "域名只能包含字母、数字、点和连字符：$DOMAIN" ;;
esac
command -v docker >/dev/null 2>&1 || die "未找到 docker，请先安装 Docker：https://docs.docker.com/engine/install/"
docker compose version >/dev/null 2>&1 || die "未找到 docker compose 插件"
command -v curl >/dev/null 2>&1 || die "未找到 curl"

mkdir -p "$DIR"
cd "$DIR"
for f in compose.yaml Caddyfile .env.example; do
  curl -fsSL "$RAW/$f" -o "$f.tmp" || die "下载 $f 失败"
  mv "$f.tmp" "$f"
done

if [ -f .env ]; then
  echo "保留已有 .env（不覆盖）"
else
  sed "s|^OWNMIND_DOMAIN=.*|OWNMIND_DOMAIN=${DOMAIN}|" .env.example > .env
fi

docker compose pull
docker compose up -d
echo
echo "完成。等 1–2 分钟让证书签发，然后打开 https://${DOMAIN} 设置主人密码。"
echo "状态：cd $DIR && docker compose ps    升级：docker compose pull && docker compose up -d"
