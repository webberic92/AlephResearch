#!/bin/bash

NODE1_STATUS="node-1/node_status"

TX_PER_ROUND=$(grep "number_of_transactions" "$NODE1_STATUS" | awk -F= '{gsub(/ /,"",$2); print $2}' | cut -d'#' -f1)
TOTAL_ROUNDS=$(grep "total_rounds" "$NODE1_STATUS" | awk -F= '{gsub(/ /,"",$2); print $2}' | cut -d'#' -f1)

if [[ -z "$TX_PER_ROUND" || -z "$TOTAL_ROUNDS" ]]; then
  echo "❌ Error: Could not extract TX_PER_ROUND or TOTAL_ROUNDS from $NODE1_STATUS"
  exit 1
fi

TOTAL_TX=$((TX_PER_ROUND * TOTAL_ROUNDS))

echo ""
echo "🚀 Transaction Throughput (TPS)"
echo "----------------------------------------------"
echo "📊 Transaction Throughput per Node"
echo "----------------------------------"
printf "%-40s %s\n" "Node" "TPS"

sum_tps=0
count=0

# Function to strip ANSI and extract ISO8601 timestamp
extract_clean_timestamp() {
  echo "$1" | sed -r 's/\x1B\[[0-9;]*[mK]//g' | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\.[0-9]+'
}

for node_dir in node-*; do
  logfile="$node_dir/node_status"
  [[ ! -f "$logfile" ]] && { echo "⚠️  Log file missing in $node_dir/"; continue; }

  first_line=$(grep "Successfully wrote finalized DAG for round 1" "$logfile" | head -1)
  last_line=$(grep "Successfully wrote finalized DAG for round $TOTAL_ROUNDS" "$logfile" | tail -1)

  [[ -z "$first_line" || -z "$last_line" ]] && { echo "⚠️  Missing round 1 or round $TOTAL_ROUNDS logs in $node_dir/"; continue; }

  ts_start=$(extract_clean_timestamp "$first_line")
  ts_end=$(extract_clean_timestamp "$last_line")

  duration=$(python3 -c "
from datetime import datetime
try:
    start = datetime.strptime('$ts_start', '%Y-%m-%dT%H:%M:%S.%f')
    end = datetime.strptime('$ts_end', '%Y-%m-%dT%H:%M:%S.%f')
    print((end - start).total_seconds())
except:
    print(-1)
")

  if (( $(echo "$duration <= 0" | bc -l) )); then
    echo "⚠️  Invalid timestamp in $node_dir/"
    continue
  fi

  tps=$(echo "scale=2; $TOTAL_TX / $duration" | bc -l)
  sum_tps=$(echo "$sum_tps + $tps" | bc -l)
  ((count++))

  printf "%-40s TPS: %.2f\n" "$node_dir/" "$tps"
done

echo "----------------------------------"

if [[ $count -gt 0 ]]; then
  avg_tps=$(echo "scale=2; $sum_tps / $count" | bc -l)
  echo "📈 Average TPS across $count nodes (Tx/Round: $TX_PER_ROUND, Rounds: $TOTAL_ROUNDS): $avg_tps"
else
  echo "⚠️  No valid data found in any nodes."
fi
