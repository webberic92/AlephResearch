#!/bin/bash

echo "🖥️  Resource Utilization per Node (Average)"
echo "---------------------------------------------------------"
printf "%-40s %-15s %-15s\n" "Node" "CPU Util (%)" "Mem Util (%)"
echo "---------------------------------------------------------"

total_cpu=0
total_mem=0
node_count=0

for node_path in node-*/; do
    cpu_file="${node_path}/cpu_usage"
    mem_file="${node_path}/mem_usage"

    if [[ -f "$cpu_file" && -f "$mem_file" ]]; then
        # CPU: extract user + system + steal
        cpu_sum=$(cat "$cpu_file" | sed $'s/\\r$//; s/[[:space:]]\\{1,\\}/ /g' | awk '
            $2 == "all" && $1 ~ /^[0-9]{2}:[0-9]{2}:[0-9]{2}$/ {
                sum += $3 + $5 + $6;
                count++;
            }
            END {
                if (count > 0) printf "%.2f", sum / count;
                else print 0;
            }
        ')

        # MEM: extract %memused from 3rd column
        mem_sum=$(cat "$mem_file" | sed $'s/\\r$//; s/[[:space:]]\\{1,\\}/ /g' | awk '
            $1 ~ /^[0-9]{2}:[0-9]{2}:[0-9]{2}$/ && $3 ~ /^[0-9.]+$/ {
                sum += $3;
                count++;
            }
            END {
                if (count > 0) printf "%.2f", sum / count;
                else print 0;
            }
        ')

        printf "%-40s %-15s %-15s\n" "$node_path" "$cpu_sum" "$mem_sum"

        total_cpu=$(echo "$total_cpu + $cpu_sum" | bc -l)
        total_mem=$(echo "$total_mem + $mem_sum" | bc -l)
        node_count=$((node_count + 1))
    else
        printf "%-40s ⚠️ Missing cpu_usage or mem_usage\n" "$node_path"
    fi
done

if [[ $node_count -gt 0 ]]; then
    avg_cpu=$(echo "scale=2; $total_cpu / $node_count" | bc)
    avg_mem=$(echo "scale=2; $total_mem / $node_count" | bc)
    echo "---------------------------------------------------------"
    printf "📊 Average CPU Utilization across %d nodes: %.2f%%\n" "$node_count" "$avg_cpu"
    printf "📊 Average Memory Utilization across %d nodes: %.2f%%\n" "$node_count" "$avg_mem"
else
    echo "❌ No valid resource usage files found."
fi
