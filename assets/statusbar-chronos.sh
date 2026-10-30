#!/usr/bin/env bash
# Waybar / Polybar status reader for Chronos (~/.cache/chronos/status.json)
STATUS_FILE="${XDG_CACHE_HOME:-$HOME/.cache}/chronos/status.json"

if [[ ! -f "$STATUS_FILE" ]]; then
  echo '{"text": "⧗ Idle", "class": "idle", "tooltip": "Chronos is not running"}'
  exit 0
fi

if command -v jq >/dev/null 2>&1; then
  STATUS=$(jq -r '.status' "$STATUS_FILE")
  ACT=$(jq -r '.activity' "$STATUS_FILE")
  CAT=$(jq -r '.category' "$STATUS_FILE")
  REM=$(jq -r '.remaining_hms' "$STATUS_FILE")
  PCT=$(jq -r '.progress_percent' "$STATUS_FILE")
  LVL=$(jq -r '.level' "$STATUS_FILE")
  echo "{\"text\": \"⧗ ${ACT} ${REM} (${PCT}%)\", \"class\": \"${STATUS}\", \"tooltip\": \"Category: ${CAT} | Level ${LVL}\"}"
else
  cat "$STATUS_FILE"
fi
