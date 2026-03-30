#!/usr/bin/env bash
# ============================================================
# git_log 工具实现
#
# 调用约定：参数以 --key value 方式传入
# 输出约定：stdout 输出合法 JSON，错误时输出 {"error": "msg"}
# ============================================================
set -euo pipefail

LIMIT=20
AUTHOR=""
PATH_FILTER=""
SINCE=""

# ── 解析参数 ─────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --limit)  LIMIT="$2";        shift 2 ;;
    --author) AUTHOR="$2";       shift 2 ;;
    --path)   PATH_FILTER="$2";  shift 2 ;;
    --since)  SINCE="$2";        shift 2 ;;
    *)        shift ;;
  esac
done

# ── 校验工作目录存在 git 仓库 ─────────────────────────────────
if ! git rev-parse --is-inside-work-tree &>/dev/null; then
  echo '{"error": "当前目录不是 Git 仓库"}'
  exit 0
fi

# ── 构建 git log 命令 ─────────────────────────────────────────
GIT_ARGS=("log" "--pretty=format:%H|%an|%ae|%ad|%s" "--date=short" "-n" "$LIMIT")

[[ -n "$AUTHOR" ]] && GIT_ARGS+=("--author=$AUTHOR")
[[ -n "$SINCE"  ]] && GIT_ARGS+=("--since=$SINCE")
[[ -n "$PATH_FILTER" ]] && GIT_ARGS+=("--" "$PATH_FILTER")

# ── 执行并转换为 JSON ─────────────────────────────────────────
OUTPUT=$(git "${GIT_ARGS[@]}" 2>/dev/null || true)

if [[ -z "$OUTPUT" ]]; then
  echo '{"commits": [], "total": 0}'
  exit 0
fi

# 逐行转换为 JSON 数组
JSON_ITEMS=()
while IFS='|' read -r hash author email date subject; do
  # 转义 subject 中的双引号和反斜线
  subject_escaped=$(printf '%s' "$subject" | sed 's/\\/\\\\/g; s/"/\\"/g')
  author_escaped=$(printf '%s' "$author"   | sed 's/\\/\\\\/g; s/"/\\"/g')
  JSON_ITEMS+=("{\"hash\":\"${hash:0:8}\",\"author\":\"$author_escaped\",\"email\":\"$email\",\"date\":\"$date\",\"subject\":\"$subject_escaped\"}")
done <<< "$OUTPUT"

# 用逗号拼接
JOINED=$(IFS=','; echo "${JSON_ITEMS[*]}")
TOTAL=${#JSON_ITEMS[@]}
echo "{\"commits\":[$JOINED],\"total\":$TOTAL}"
