#!/usr/bin/env bash
# ============================================================
# on_agent_start.sh
# 触发事件：agent.start（fire_and_forget）
#
# 环境变量（由 Agent 注入）：
#   AGENT_EVENT — 完整事件 payload（JSON 字符串）
#   LOG_PREFIX  — 来自 hook.toml [env] 节
#
# payload.data 结构：
#   { "project_dir": "/path/to/project", "mode": "cli" }
# ============================================================
set -euo pipefail

LOG_FILE="${LOG_FILE:-/tmp/project-stats.log}"
PREFIX="${LOG_PREFIX:-[project-stats]}"
TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')

# 从 AGENT_EVENT 解析字段（依赖 jq；不可用时优雅降级）
SESSION_ID="unknown"
PROJECT_DIR="unknown"
if command -v jq &>/dev/null && [[ -n "${AGENT_EVENT:-}" ]]; then
  SESSION_ID=$(echo "$AGENT_EVENT" | jq -r '.session_id // "none"')
  PROJECT_DIR=$(echo "$AGENT_EVENT" | jq -r '.data.project_dir // "unknown"')
fi

# 写入日志
echo "$TIMESTAMP $PREFIX [agent.start] session=$SESSION_ID project=$PROJECT_DIR" \
  >> "$LOG_FILE"

# 保持日志不超过上限（简单 tail 截断）
MAX_LINES="${MAX_LOG_LINES:-500}"
if [[ -f "$LOG_FILE" ]]; then
  LINE_COUNT=$(wc -l < "$LOG_FILE")
  if (( LINE_COUNT > MAX_LINES )); then
    tail -n "$MAX_LINES" "$LOG_FILE" > "${LOG_FILE}.tmp" && mv "${LOG_FILE}.tmp" "$LOG_FILE"
  fi
fi

# fire_and_forget 模式不需要输出任何内容
exit 0
