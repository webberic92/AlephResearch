#!/bin/bash

LOG_DIR="./"  # Assuming you're in the logs directory
TX_PER_ROUND=5
ROUNDS=5
TOTAL_TX=$((TX_PER_ROUND * ROUNDS))

echo "📊 Transaction Throughput per Node"
echo "----------------------------------"

total_tps=0
node_count=0

for node_path in node-*/; do
    file_path="${node_path}/node_status"

    # Some files might end in .txt
    if [[ ! -f "$file_path" ]]; then
        file_path="${node_path}/node_status.txt"
    fi

    if [[ -f "$file_path" ]]; then
        # Extract timestamps, strip color codes, and sort
        timestamps=$(grep "Successfully wrote finalized DAG for round" "$file_path" | \
                     sed 's/\x1b\[[0-9;]*m//g' | \
                     awk '{print $1}' | sed 's/Z//' )

        start_time=$(echo "$timestamps" | head -n 1)
        end_time=$(echo "$timestamps" | tail -n 1)

        # Convert timestamps to epoch (with nanoseconds)
        start_epoch=$(LC_ALL=C date -d "$start_time" +%s.%N 2>/dev/null)
        end_epoch=$(LC_ALL=C date -d "$end_time" +%s.%N 2>/dev/null)

        if [[ -n "$start_epoch" && -n "$end_epoch" ]]; then
            duration=$(echo "$end_epoch - $start_epoch" | bc -l)
            tps=$(echo "$TOTAL_TX / $duration" | bc -l)

            printf "%-40s TPS: %.2f\n" "$node_path" "$tps"

            total_tps=$(echo "$total_tps + $tps" | bc -l)
            node_count=$((node_count + 1))
        else
            printf "%-40s ⚠️ Invalid timestamps, skipping\n" "$node_path"
        fi
    fi
done

# Compute and print average TPS
if [[ $node_count -gt 0 ]]; then
    avg_tps=$(echo "$total_tps / $node_count" | bc -l)
    echo "----------------------------------"
printf "📈 Average TPS across %d nodes (Tx/Round: %d, Rounds: %d): %.2f\n" "$node_count" "$TX_PER_ROUND" "$ROUNDS" "$avg_tps"
fi
