#!/bin/bash

LOG_DIR="./"
NODE1_STATUS="node-1/node_status"

# Extract config values from node-1
TX_PER_ROUND=$(grep "number_of_transactions" "$NODE1_STATUS" | awk -F= '{gsub(/ /,"",$2); print $2}' | cut -d'#' -f1)
ROUNDS=$(grep "total_rounds" "$NODE1_STATUS" | awk -F= '{gsub(/ /,"",$2); print $2}' | cut -d'#' -f1)
TOTAL_TX=$((TX_PER_ROUND * ROUNDS))

echo "📊 Transaction Throughput per Node"
echo "----------------------------------"
printf "%-40s %s\n" "Node" "TPS"

total_tps=0
node_count=0

for node_path in node-* node-ip-*; do
    file_path="${node_path}/node_status"
    [[ ! -f "$file_path" ]] && file_path="${node_path}/node_status.txt"
    [[ ! -f "$file_path" ]] && continue

    cleaned_lines=$(sed -E 's/\x1B\[[0-9;]*[mK]//g' "$file_path" | tr -cd '\11\12\15\40-\176')

    rounds_logged=$(echo "$cleaned_lines" | grep -o "Finalized round [0-9]\+" | awk '{print $3}' | sort -n | uniq)
    highest_round=$(echo "$rounds_logged" | tail -n 1)

    if [[ "$highest_round" == "1" ]]; then
        # Get start from first valid timestamp (anywhere early)
        init_ts=$(echo "$cleaned_lines" | head -n 50 | grep -Eo '^[0-9]{4}-[^ ]+' | head -n 1 | sed 's/Z//' )
        # Get commit time from Finalized round 1
        commit_ts=$(echo "$cleaned_lines" | grep "Finalized round 1" | head -n 1 | awk '{print $1}' | sed 's/Z//')
        [[ -z "$init_ts" || -z "$commit_ts" ]] && { printf "%-40s ⚠️ Missing init or commit ts\n" "$node_path"; continue; }

        start_epoch=$(date -u -d "$init_ts" +"%s.%N" 2>/dev/null)
        end_epoch=$(date -u -d "$commit_ts" +"%s.%N" 2>/dev/null)
    else
        start_ts=$(echo "$cleaned_lines" | grep "Finalized round 1" | head -n1 | awk '{print $1}' | sed 's/Z//')
        end_ts=$(echo "$cleaned_lines" | grep "Finalized round $ROUNDS" | tail -n1 | awk '{print $1}' | sed 's/Z//')
        [[ -z "$start_ts" || -z "$end_ts" ]] && { printf "%-40s ⚠️ Missing round 1 or $ROUNDS finalized ts\n" "$node_path"; continue; }

        start_epoch=$(date -u -d "$start_ts" +"%s.%N" 2>/dev/null)
        end_epoch=$(date -u -d "$end_ts" +"%s.%N" 2>/dev/null)
    fi

    if [[ -n "$start_epoch" && -n "$end_epoch" ]]; then
        duration=$(echo "$end_epoch - $start_epoch" | bc -l)
        [[ $(echo "$duration <= 0" | bc -l) -eq 1 ]] && duration="0.001"

        tps=$(echo "$TOTAL_TX / $duration" | bc -l)
        tps=$(printf "%.2f" "$tps")

        printf "%-40s TPS: %s\n" "$node_path" "$tps"
        total_tps=$(echo "$total_tps + $tps" | bc -l)
        node_count=$((node_count + 1))
    else
        printf "%-40s ⚠️ Invalid timestamps, skipping\n" "$node_path"
    fi
done

if [[ $node_count -gt 0 ]]; then
    avg_tps=$(echo "$total_tps / $node_count" | bc -l)
    echo "----------------------------------"
    printf "📈 Average TPS across %d nodes (Tx/Round: %d, Rounds: %d): %.2f\n" \
        "$node_count" "$TX_PER_ROUND" "$ROUNDS" "$avg_tps"
else
    echo "⚠️  No valid data found in any nodes."
fi
