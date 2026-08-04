#!/usr/bin/env bash
# 跨域 feature 不得调用他域 repository 或 kernel Port 的变更方法。
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MUTATE_RE='(Port|Repository)::(create|update|delete|update_status)\('

fail_if_match() {
  local dir=$1
  shift
  local name
  for name in "$@"; do
    if rg -q "${name}${MUTATE_RE}" "${dir}" 2>/dev/null; then
      echo "error: ${dir} 不得调用他域 ${name} 变更方法（请用 kernel 只读 port 或队列）"
      rg -n "${name}${MUTATE_RE}" "${dir}"
      exit 1
    fi
  done
}

# identity 域不得调用其他域的变更方法
fail_if_match "${ROOT}/features/identity" \
  FilePort FileRepository

# file 域不得调用其他域的变更方法
fail_if_match "${ROOT}/features/file" \
  AccountPort AccountRepository

echo "check_cross_domain_repository: ok"
