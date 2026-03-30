#!/usr/bin/env bash
# ============================================================
# on_router_decision.sh
# 触发事件：router.decision（intercepting）
#
# 环境变量（由 Agent 注入）：
#   AGENT_EVENT — 完整事件 payload（JSON 字符串）
#
# payload.data 结构：
#   {
#     "proposed_mode":        "basic_loop | plan_and_execute | full_pipeline",
#     "classification_source": "forced | auto",
#     "task_preview":          "任务前 200 字"
#   }
#
# 返回约定（intercepting 模式）：
#   - 空输出 / 非 JSON        → Continue（接受内置决策）
#   - {"override_mode": "X"} → 覆盖为 X
#   - {"cancel": "reason"}   → Cancel（阻止执行，返回错误）
#
# 本脚本示例：
#   检测任务预览中包含"生产"/"production"/"deploy"等高风险关键词时
#   将模式升级为 full_pipeline，确保有 Checker 验证。
# ============================================================
set -euo pipefail

# 无 jq 则直接退出（Continue）
if ! command -v jq &>/dev/null; then
  exit 0
fi

PROPOSED=$(echo "$AGENT_EVENT" | jq -r '.data.proposed_mode        // "basic_loop"')
SOURCE=$(  echo "$AGENT_EVENT" | jq -r '.data.classification_source // "auto"')
PREVIEW=$( echo "$AGENT_EVENT" | jq -r '.data.task_preview          // ""' | tr '[:upper:]' '[:lower:]')

# ── 策略 1：强制覆盖已经被 /mode 锁定的决策时不干预 ─────────────
if [[ "$SOURCE" == "forced" ]]; then
  exit 0
fi

# ── 策略 2：已经是最高级别则不干预 ───────────────────────────────
if [[ "$PROPOSED" == "full_pipeline" ]]; then
  exit 0
fi

# ── 策略 3：检测高风险关键词，升级为 full_pipeline ────────────────
HIGH_RISK_KEYWORDS=(
  "生产" "production" "prod" "deploy" "部署"
  "删除所有" "drop table" "rm -rf" "truncate"
  "迁移" "migrate" "migration"
)

for keyword in "${HIGH_RISK_KEYWORDS[@]}"; do
  if [[ "$PREVIEW" == *"$keyword"* ]]; then
    # 返回 JSON 覆盖决策
    echo "{\"override_mode\": \"full_pipeline\"}"
    exit 0
  fi
done

# ── 其他情况：不干预（Continue） ────────────────────────────────
exit 0
