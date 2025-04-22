#!/bin/bash

echo "📊 Transaction Throughput per Node (Final Round Only)"
echo "---------------------------------------------------------"
printf "%-10s %-20s %-10s\n" "Node" "Epoch Time" "Human Time"
echo "---------------------------------------------------------"

total_epoch=0
node_count=0

for node_dir in node-*; do
  logfile="$node_dir/node_status"

  if [[ ! -f "$logfile" ]]; then
    continue
  fi

  raw_line=$(grep "Successfully wrote finalized DAG for round 1" "$logfile" | tail -1)
  clean_line=$(echo "$raw_line" | sed -r 's/\x1B\[[0-9;]*[mK]//g')
  timestamp=$(echo "$clean_line" | awk '{print $1}' | sed 's/T/ /')

  epoch_time=$(date -d "$timestamp" +"%s" 2>/dev/null)
  human_time=$(date -d "$timestamp" +"%H:%M:%S" 2>/dev/null)

  if [[ -n "$epoch_time" ]]; then
    printf "%-10s %-20s %-10s\n" "$node_dir" "$epoch_time" "$human_time"
    total_epoch=$((total_epoch + epoch_time))
    node_count=$((node_count + 1))
  fi
done

echo "---------------------------------------------------------"

if [[ $node_count -gt 0 ]]; then
  avg_epoch=$(echo "scale=2; $total_epoch / $node_count" | bc)
  avg_time=$(date -d "@${avg_epoch%.*}" +"%Y-%m-%d %H:%M:%S")
  echo "📈 Average Finalization Time across $node_count nodes: $avg_time (Epoch: $avg_epoch)"
else
  echo "⚠️  No valid logs found to compute throughput."
fi
