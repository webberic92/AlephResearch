# Script to automatically shut down EC2 instances after tests.
# shutdown_script.py
# What’s Missing/Needed from You:

#     Instance IDs: Confirm or provide the list of instance IDs that need to be stopped.
#     Script Path: Provide the path where shutdown_script.py will be located if using the SSM Document method.
#     Scheduling Requirements: Specify the schedule for running the shutdown script if different from the example (daily at midnight).



import boto3
import os

# Configuration
instance_ids = os.environ['INSTANCE_IDS'].split(',')  # Set environment variable with comma-separated instance IDs

# Initialize a session using Amazon EC2
ec2_client = boto3.client('ec2')

def shutdown_instances():
    response = ec2_client.stop_instances(
        InstanceIds=instance_ids,
        Force=True  # Optionally force stop instances
    )
    print(f"Stopping instances: {response}")

if __name__ == "__main__":
    shutdown_instances()
