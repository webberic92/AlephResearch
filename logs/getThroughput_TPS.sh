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

for node_dir in node-*; do
  logfile="$node_dir/node_status"
  if [[ ! -f "$logfile" ]]; then
    echo "⚠️  Log file missing in $node_dir/"
    continue
  fi

  first_line=$(grep "Successfully wrote finalized DAG for round 1" "$logfile" | head -1)
  last_line=$(grep "Successfully wrote finalized DAG for round $TOTAL_ROUNDS" "$logfile" | tail -1)

  if [[ -z "$first_line" || -z "$last_line" ]]; then
    echo "⚠️  Missing round 1 or round $TOTAL_ROUNDS logs in $node_dir/"
    continue
  fi

  ts_start=$(echo "$first_line" | grep -oE '^[0-9T:\.\-]+Z' | sed 's/T/ /;s/Z//')
  ts_end=$(echo "$last_line" | grep -oE '^[0-9T:\.\-]+Z' | sed 's/T/ /;s/Z//')

  epoch_start=$(date -d "$ts_start" +%s.%N 2>/dev/null)
  epoch_end=$(date -d "$ts_end" +%s.%N 2>/dev/null)

  if [[ -z "$epoch_start" || -z "$epoch_end" ]]; then
    echo "⚠️  Invalid timestamp in $node_dir/"
    continue
  fi

  duration=$(echo "$epoch_end - $epoch_start" | bc -l)
  cmp_zero=$(echo "$duration <= 0" | bc -l)
  [[ $cmp_zero -eq 1 ]] && duration=0.001  # minimum 1ms

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
