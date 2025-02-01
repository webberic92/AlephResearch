#!/bin/bash

# Define variables
TARGET="x86_64-unknown-linux-musl"
S3_BUCKET="aleph-research"
BINARY_1="aleph_rbc"
# BINARY_2="aleph_start"
IP_SERVER_SCRIPT="IpServer.py"

# Step 1: Build the project
echo "Building the project in release mode for target $TARGET..."
cargo build --release --target $TARGET
if [ $? -ne 0 ]; then
    echo "Build failed. Exiting."
    exit 1
fi

# Step 2: Upload binaries to S3
echo "Uploading $BINARY_1 to S3 bucket $S3_BUCKET..."
aws s3 cp "target/$TARGET/release/$BINARY_1" "s3://$S3_BUCKET/"
if [ $? -ne 0 ]; then
    echo "Failed to upload $BINARY_1. Exiting."
    exit 1
fi
echo "Uploaded $BINARY_1 with checksum: $(md5sum target/$TARGET/release/$BINARY_1)"

# echo "Uploading $BINARY_2 to S3 bucket $S3_BUCKET..."
# aws s3 cp "target/$TARGET/release/$BINARY_2" "s3://$S3_BUCKET/"
# if [ $? -ne 0 ]; then
#     echo "Failed to upload $BINARY_2. Exiting."
#     exit 1
# fi
# echo "Uploaded $BINARY_2 with checksum: $(md5sum target/$TARGET/release/$BINARY_2)"


# Step 3: Upload IpServer.py to S3
echo "Uploading $IP_SERVER_SCRIPT to S3 bucket $S3_BUCKET..."
aws s3 cp "src/$IP_SERVER_SCRIPT" "s3://$S3_BUCKET/"
if [ $? -ne 0 ]; then
    echo "Failed to upload $IP_SERVER_SCRIPT. Exiting."
    exit 1
fi

echo "Deployment completed successfully! Run 'cdk deploy'"