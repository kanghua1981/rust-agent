#!/usr/bin/env bash
# ============================================================
# word_count 工具实现
#
# 统计指定路径下的代码行数、单词数、字符数
# 输出合法 JSON
# ============================================================
set -euo pipefail

TARGET_PATH=""
EXT=""
EXCLUDE="target|node_modules|.git|dist|build"

# ── 解析参数 ─────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
  case "$1" in
    --path)    TARGET_PATH="$2"; shift 2 ;;
    --ext)     EXT="$2";         shift 2 ;;
    --exclude) EXCLUDE="$2";     shift 2 ;;
    *)         shift ;;
  esac
done

if [[ -z "$TARGET_PATH" ]]; then
  echo '{"error": "path 参数不能为空"}'
  exit 0
fi

if [[ ! -e "$TARGET_PATH" ]]; then
  echo "{\"error\": \"路径不存在: $TARGET_PATH\"}"
  exit 0
fi

# ── 收集文件列表 ──────────────────────────────────────────────
if [[ -f "$TARGET_PATH" ]]; then
  FILES=("$TARGET_PATH")
else
  # 目录：用 find 递归
  FIND_ARGS=("$TARGET_PATH" "-type" "f")
  if [[ -n "$EXT" ]]; then
    FIND_ARGS+=("-name" "*.${EXT}")
  fi
  # 排除指定目录
  mapfile -t FILES < <(
    find "${FIND_ARGS[@]}" 2>/dev/null \
      | grep -Ev "/(${EXCLUDE//,/|})/?" \
      || true
  )
fi

TOTAL_FILES=${#FILES[@]}
if [[ $TOTAL_FILES -eq 0 ]]; then
  echo '{"lines":0,"words":0,"chars":0,"files":0}'
  exit 0
fi

# ── 统计 ──────────────────────────────────────────────────────
read -r LINES WORDS CHARS _ < <(
  wc -lwm "${FILES[@]}" 2>/dev/null | tail -1 || echo "0 0 0 total"
)

# 去掉可能的前导空格
LINES=$(echo "$LINES" | tr -d ' ')
WORDS=$(echo "$WORDS" | tr -d ' ')
CHARS=$(echo "$CHARS" | tr -d ' ')

# ── 输出 JSON ─────────────────────────────────────────────────
echo "{\"lines\":$LINES,\"words\":$WORDS,\"chars\":$CHARS,\"files\":$TOTAL_FILES}"
