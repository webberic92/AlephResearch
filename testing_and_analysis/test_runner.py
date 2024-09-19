# Script or program to run tests, start nodes, and initiate transactions.
# test_runner.py

# Notes:
#     Database Credentials: Ensure the environment variables for database connection details are set correctly.
#     Queries: Modify SQL queries according to your actual log schema.
#     CSV Files: The output CSV files can be imported into BI tools like Amazon QuickSight or Excel for further analysis and visualization.

import boto3
import os
import time
# Configuration
instance_ids = os.environ['INSTANCE_IDS'].split(',')  # Comma-separated list of EC2 instance IDs
transaction_count = 256
transaction_size = 250  # in bytes
command = "start_test_command"  # Replace with the actual command to start the test

# Initialize a session using the default profile (or set your own AWS access keys if needed)
session = boto3.Session(
    aws_access_key_id=os.getenv("AWS_ACCESS_KEY_ID"),  # Optional
    aws_secret_access_key=os.getenv("AWS_SECRET_ACCESS_KEY"),  # Optional
    region_name=os.getenv("AWS_REGION")
)

# Initialize an EC2 client
ec2_client = session.client('ec2')

def start_test():
    for instance_id in instance_ids:
        print(f"Starting test on instance {instance_id}")
        response = ec2_client.send_command(
            InstanceIds=[instance_id],
            DocumentName="AWS-RunShellScript",
            Parameters={
                'commands': [command]
            }
        )
        print(f"Command sent to instance {instance_id}, response: {response}")
        time.sleep(10)  # Wait for a while before sending the next command to avoid throttling

def monitor_test():
    # Example function to monitor the test; customize based on your needs
    print("Monitoring test...")
    time.sleep(600)  # Wait for 10 minutes; adjust as needed

if __name__ == "__main__":
    start_test()
    monitor_test()
