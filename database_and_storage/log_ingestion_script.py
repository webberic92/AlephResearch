import psycopg2
import os
from datetime import datetime

# Database configuration from environment variables
db_endpoint = os.getenv('DB_ENDPOINT')
db_port = os.getenv('DB_PORT')
db_name = os.getenv('DB_NAME')
db_user = os.getenv('DB_USER')
db_password = os.getenv('DB_PASSWORD')

log_file_path = '/var/log/aleph/'  # Example log file path on EC2 instance

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
    # Ingest log files from the specified path
    for log_file in os.listdir(log_file_path):
        with open(os.path.join(log_file_path, log_file), 'r') as file:
            logs = file.readlines()
            for log in logs:
                # Insert logs into the database
                cursor.execute(
                    "INSERT INTO logs (log_type, log_message, timestamp) VALUES (%s, %s, %s)",
                    (log_file, log, datetime.now())  # Replace timestamp logic if needed
                )
    conn.commit()

if __name__ == "__main__":
    ingest_logs()
    cursor.close()
    conn.close()
