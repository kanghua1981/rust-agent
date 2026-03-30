#!/usr/bin/env bash
# ============================================================
# on_tool_after.sh
# 触发事件：tool.after（fire_and_forget）
#
# 环境变量（由 Agent 注入）：
#   AGENT_EVENT  — 完整事件 payload（JSON 字符串）
#   AUDIT_LOG    — 来自 hook.toml [env] 节
#
# payload.data 结构：
#   {
#     "tool_name":      "write_file",
#     "success":        true,
#     "output_preview": "前 200 字的输出"
#   }
# ============================================================
set -euo pipefail

AUDIT_LOG="${AUDIT_LOG:-/tmp/project-stats-audit.log}"
TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')

TOOL_NAME="unknown"
SUCCESS="unknown"
SESSION_ID="unknown"

if command -v jq &>/dev/null && [[ -n "${AGENT_EVENT:-}" ]]; then
  TOOL_NAME=$(echo  "$AGENT_EVENT" | jq -r '.data.tool_name  // "unknown"')
  SUCCESS=$(echo    "$AGENT_EVENT" | jq -r '.data.success     // "unknown"')
  SESSION_ID=$(echo "$AGENT_EVENT" | jq -r '.session_id      // "none"')
fi

STATUS="OK"
[[ "$SUCCESS" == "false" ]] && STATUS="FAIL"

echo "$TIMESTAMP [tool.after] $STATUS tool=$TOOL_NAME session=$SESSION_ID" \
  >> "$AUDIT_LOG"

exit 0
