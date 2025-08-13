#!/bin/bash

# Settings
BUCKET_NAME="aleph-research"
DEST_DIR="/tmp/buckets"
SCRIPT_DIR="/home/webbrico/backup2025/AlephResearch/logs/test"

# Ensure destination exists
mkdir -p "$DEST_DIR"

# Find only log folders matching the test naming pattern
folders=$(aws s3 ls "s3://$BUCKET_NAME/2048_RSA/logs/" | awk '/PRE/ {print $2}' | grep 'RSA_N')

# Loop through matching folders
for folder in $folders; do
    folder_name="${folder%/}"  # remove trailing slash
    echo "Processing: $folder_name"

    # Local path to copy to
    LOCAL_PATH="$DEST_DIR/$folder_name"
    mkdir -p "$LOCAL_PATH"

    # Copy S3 folder recursively
    aws s3 cp "s3://$BUCKET_NAME/2048_RSA/logs/$folder_name/" "$LOCAL_PATH" --recursive

    # Run the report script inside copied folder, using full path to the original tools
    cp "$SCRIPT_DIR"/*.sh "$LOCAL_PATH"
    cd "$LOCAL_PATH" || continue
    bash ./generateReport.sh

    # Rename the output if it exists
    if [[ -f "final_metrics_report.txt" ]]; then
        if [[ "$folder_name" =~ ([a-z0-9.]+)_RSA_([0-9]+)nodes_([0-9]+)txpb_([0-9]+)rounds ]]; then
            instance="${BASH_REMATCH[1]}"
            nodes="${BASH_REMATCH[2]}"
            txpb="${BASH_REMATCH[3]}"
            rounds="${BASH_REMATCH[4]}"
            new_name="${instance}_${nodes}nodes_${txpb}txpb_rounds${rounds}_RSA.txt"
            mv final_metrics_report.txt "$new_name"
            echo "✅ Renamed to: $new_name"
        else
            echo "⚠️ Could not parse naming from $folder_name"
        fi
    else
        echo "⚠️ No report generated in $folder_name"
    fi
done
