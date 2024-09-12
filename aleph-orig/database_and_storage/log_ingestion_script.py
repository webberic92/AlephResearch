# Script for periodic log ingestion from EC2 to RDS database.
# log_ingestion_script.py
# What’s Missing/Needed from You:

#     Database Credentials: Confirm or provide preferred username and password for the PostgreSQL database.
#     Log Schema: Provide details on the log schema (e.g., table structure) for accurate SQL insert commands.
#     Log File Format: Specify the format of logs to tailor the ingestion logic accordingly.
#     Environment Variable Setup: Ensure that environment variables for database credentials are properly set up on the EC2 instances where the script runs.
import psycopg2
import boto3
import os

# Configuration
db_endpoint = os.environ['DB_ENDPOINT']  # Set environment variables for security
db_port = os.environ['DB_PORT']
db_name = os.environ['DB_NAME']
db_user = os.environ['DB_USER']
db_password = os.environ['DB_PASSWORD']
log_file_path = '/var/log/aleph/'

# Establish connection to PostgreSQL
conn = psycopg2.connect(
    dbname=db_name,
    user=db_user,
    password=db_password,
    host=db_endpoint,
    port=db_port
)
cursor = conn.cursor()

def ingest_logs():
    # Example function to ingest logs from the specified file paths
    for log_file in os.listdir(log_file_path):
        with open(os.path.join(log_file_path, log_file), 'r') as file:
            logs = file.readlines()
            for log in logs:
                # Example SQL for inserting logs; adjust according to your schema
                cursor.execute(
                    "INSERT INTO logs (log_type, log_message, timestamp) VALUES (%s, %s, %s)",
                    (log_file, log, 'timestamp')  # Replace 'timestamp' with actual timestamp if available
                )
    conn.commit()

if __name__ == "__main__":
    ingest_logs()
    cursor.close()
    conn.close()