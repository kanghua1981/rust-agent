#!/usr/bin/env bash
# ============================================================
# probe_on_start.sh
# 触发事件：agent.start（blocking）
#
# 环境变量（由 Agent 注入）：
#   AGENT_EVENT   — 完整事件 payload（JSON）
#   PROBE_TIMEOUT — 来自 hook.toml [env]，curl 超时秒数
#   RESULT_FILE   — 探测结果写入路径
#
# blocking 模式：Agent 等待本脚本退出后才接受第一条用户消息。
# 务必设置合理的 timeout（在 .toml 中配置），脚本超时会被强制终止。
#
# 输出协议：
#   blocking 模式不需要返回 JSON，stdout 仅用于日志。
#   探测结果写到 RESULT_FILE，供后续工具或 hook 读取。
# ============================================================
set -euo pipefail

TIMEOUT="${PROBE_TIMEOUT:-3}"
RESULT_FILE="${RESULT_FILE:-/tmp/dev-cluster-nodes.json}"
TIMESTAMP=$(date '+%Y-%m-%d %H:%M:%S')

SESSION_ID="unknown"
if command -v jq &>/dev/null && [[ -n "${AGENT_EVENT:-}" ]]; then
  SESSION_ID=$(echo "$AGENT_EVENT" | jq -r '.session_id // "none"')
fi

echo "[$TIMESTAMP] [dev-cluster] probe_on_start: session=$SESSION_ID" >&2

# ── 已知 peer 列表 ────────────────────────────────────────────
declare -A PEERS=(
  ["gpu-box"]="http://192.168.1.100:9527"
  ["ci-runner"]="http://ci.internal:9527"
)

RESULTS=()

for NAME in "${!PEERS[@]}"; do
  BASE_URL="${PEERS[$NAME]}"
  HEALTH="${BASE_URL}/health"

  HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    --max-time "$TIMEOUT" "$HEALTH" 2>/dev/null || echo "000")

  if [[ "$HTTP_CODE" == "200" ]]; then
    STATUS="online"
  else
    STATUS="offline"
  fi

  echo "  peer $NAME → $STATUS (HTTP $HTTP_CODE)" >&2
  RESULTS+=("{\"name\":\"$NAME\",\"url\":\"$BASE_URL\",\"status\":\"$STATUS\"}")
done

# ── 写结果文件 ────────────────────────────────────────────────
JOINED=$(IFS=','; echo "${RESULTS[*]:-}")
echo "{\"probed_at\":\"$TIMESTAMP\",\"session\":\"$SESSION_ID\",\"peers\":[$JOINED]}" \
  > "$RESULT_FILE"

echo "[$TIMESTAMP] [dev-cluster] probe results saved to $RESULT_FILE" >&2
exit 0
