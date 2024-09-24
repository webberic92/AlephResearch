#!/bin/bash

# Variables
ROLE_ARN="arn:aws:iam::532911864509:role/cdk-hnb659fds-cfn-exec-role-532911864509-us-east-1"
SESSION_NAME="cdk-session"
REGION="us-east-1"

# Assume the role using AWS CLI and get the credentials
echo "Assuming IAM role: $ROLE_ARN"
ASSUME_ROLE_OUTPUT=$(aws sts assume-role --role-arn $ROLE_ARN --role-session-name $SESSION_NAME --region $REGION)

if [ $? -ne 0 ]; then
    echo "Error assuming role. Exiting."
    exit 1
fi

# Extract the credentials from the JSON output
export AWS_ACCESS_KEY_ID=$(echo $ASSUME_ROLE_OUTPUT | jq -r '.Credentials.AccessKeyId')
export AWS_SECRET_ACCESS_KEY=$(echo $ASSUME_ROLE_OUTPUT | jq -r '.Credentials.SecretAccessKey')
export AWS_SESSION_TOKEN=$(echo $ASSUME_ROLE_OUTPUT | jq -r '.Credentials.SessionToken')

echo "Successfully assumed the role."

# Optional: Verify AWS credentials are set
aws sts get-caller-identity

# Now use the AWS CDK commands with the assumed role
# Example CDK deploy command
echo "Running AWS CDK commands..."
cdk deploy --region $REGION

# Clean up environment variables after the script runs (optional)
unset AWS_ACCESS_KEY_ID
unset AWS_SECRET_ACCESS_KEY
unset AWS_SESSION_TOKEN

echo "Script execution completed."
