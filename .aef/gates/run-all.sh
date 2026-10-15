#!/usr/bin/env bash
# 先证明每个门禁会拦截违规样例，再检查仓库本身。
set -u
root="${1:-.}"; fail=0
for g in "$(dirname "$0")"/*/; do
  name=$(basename "$g"); [ "$name" = "_example" ] && continue
  for bad in "$g"fixtures/bad/*; do
    [ -e "$bad" ] || { echo "✗ $name 缺少 bad 样例"; fail=1; continue; }
    if "$g"run "$bad" >/dev/null 2>&1; then echo "✗ $name 没有拦住 $(basename "$bad")"; fail=1; fi
  done
  if "$g"run "$root"; then echo "✓ $name"; else echo "✗ $name"; fail=1; fi
done
exit $fail
