#!/usr/bin/env bash
# ============================================================
# probe_nodes 工具实现
#
# 调用约定：--key value 参数
# 输出约定：stdout 输出合法 JSON
#
# 探测逻辑：对已知 peer URL 发 HTTP GET /health，
# 检测返回码和延迟。本地节点检查 workdir 是否存在。
# ============================================================
set -euo pipefail

TARGETS=""
TIMEOUT=5

while [[ $# -gt 0 ]]; do
  case "$1" in
    --targets) TARGETS="$2"; shift 2 ;;
    --timeout) TIMEOUT="$2"; shift 2 ;;
    *) shift ;;
  esac
done

# ── 已知 peer 列表（从环境变量 PEER_URLS 读取，或使用 hardcoded 默认值）──
# 格式：name=url,name=url,...
PEER_URLS="${PEER_URLS:-gpu-box=ws://192.168.1.100:9527,ci-runner=ws://ci.internal:9527}"

# ── 已知本地节点 workdir 列表 ─────────────────────────────────
LOCAL_NODES="${LOCAL_NODES:-frontend=/workspace/frontend,backend=/workspace/backend,infra=/workspace/infra}"

RESULTS=()

# ── 探测本地节点（检查 workdir 是否存在）────────────────────────
while IFS=',' read -ra PAIRS; do
  for pair in "${PAIRS[@]}"; do
    NODE_NAME="${pair%%=*}"
    NODE_DIR="${pair##*=}"
    [[ -z "$NODE_NAME" || -z "$NODE_DIR" ]] && continue

    # 若指定了 targets，跳过不在列表中的节点
    if [[ -n "$TARGETS" ]] && [[ "$TARGETS" != *"$NODE_NAME"* ]]; then
      continue
    fi

    if [[ -d "$NODE_DIR" ]]; then
      STATUS="ok"
      NOTE="workdir exists"
    else
      STATUS="warn"
      NOTE="workdir not found: $NODE_DIR"
    fi
    RESULTS+=("{\"node\":\"$NODE_NAME\",\"type\":\"local\",\"status\":\"$STATUS\",\"note\":\"$NOTE\"}")
  done
done <<< "$LOCAL_NODES"

# ── 探测远程 peer（将 ws:// 换成 http:// 访问 /health）────────
while IFS=',' read -ra PAIRS; do
  for pair in "${PAIRS[@]}"; do
    NODE_NAME="${pair%%=*}"
    NODE_URL="${pair##*=}"
    [[ -z "$NODE_NAME" || -z "$NODE_URL" ]] && continue

    if [[ -n "$TARGETS" ]] && [[ "$TARGETS" != *"$NODE_NAME"* ]]; then
      continue
    fi

    # ws:// → http://
    HTTP_URL="${NODE_URL/ws:\/\//http://}"
    HTTP_URL="${HTTP_URL/wss:\/\//https://}"
    HEALTH_URL="${HTTP_URL%/}/health"

    START_MS=$(date +%s%N 2>/dev/null || echo "0")
    HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" \
      --max-time "$TIMEOUT" "$HEALTH_URL" 2>/dev/null || echo "000")
    END_MS=$(date +%s%N 2>/dev/null || echo "0")
    LATENCY_MS=$(( (END_MS - START_MS) / 1000000 ))

    if [[ "$HTTP_CODE" == "200" ]]; then
      STATUS="ok"
      NOTE="latency ${LATENCY_MS}ms"
    else
      STATUS="unreachable"
      NOTE="HTTP $HTTP_CODE"
    fi
    RESULTS+=("{\"node\":\"$NODE_NAME\",\"type\":\"peer\",\"url\":\"$NODE_URL\",\"status\":\"$STATUS\",\"note\":\"$NOTE\"}")
  done
done <<< "$PEER_URLS"

# ── 输出 JSON ─────────────────────────────────────────────────
COUNT=${#RESULTS[@]}
if [[ $COUNT -eq 0 ]]; then
  echo '{"nodes":[],"total":0}'
  exit 0
fi

JOINED=$(IFS=','; echo "${RESULTS[*]}")
echo "{\"nodes\":[$JOINED],\"total\":$COUNT}"
