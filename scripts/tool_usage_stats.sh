#!/bin/bash
# Simple tool usage statistics from Julie logs

LOG_FILE=".julie/logs/julie.log.$(date +%Y-%m-%d)"

if [ ! -f "$LOG_FILE" ]; then
    echo "❌ Log file not found: $LOG_FILE"
    echo "💡 Make sure you're running this from the Julie workspace directory"
    exit 1
fi

echo "📊 Julie Tool Usage Statistics"
echo "================================"
echo ""
echo "Analyzing: $LOG_FILE"
echo ""

# Extract tool names and count usage
grep "🛠️  Executing tool:" "$LOG_FILE" | \
    sed 's/.*Executing tool: //' | \
    sort | \
    uniq -c | \
    sort -rn | \
    awk '{printf "%3d  %s\n", $1, $2}'

echo ""
echo "Total tool calls:"
grep -c "🛠️  Executing tool:" "$LOG_FILE"
