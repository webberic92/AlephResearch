# SQL queries and BI tool configurations for analyzing test results.
# results_analysis.py

import psycopg2
import os
import pandas as pd

# Configuration
db_endpoint = os.environ['DB_ENDPOINT']
db_port = os.environ['DB_PORT']
db_name = os.environ['DB_NAME']
db_user = os.environ['DB_USER']
db_password = os.environ['DB_PASSWORD']

# SQL Queries
throughput_query = """
SELECT timestamp, AVG(throughput) as avg_throughput
FROM logs
GROUP BY timestamp
ORDER BY timestamp;
"""

latency_query = """
SELECT timestamp, AVG(latency) as avg_latency
FROM logs
GROUP BY timestamp
ORDER BY timestamp;
"""

# Connect to the database
conn = psycopg2.connect(
    dbname=db_name,
    user=db_user,
    password=db_password,
    host=db_endpoint,
    port=db_port
)
cursor = conn.cursor()

def fetch_data(query):
    cursor.execute(query)
    data = cursor.fetchall()
    return data

def save_to_csv(data, filename):
    df = pd.DataFrame(data, columns=['timestamp', 'value'])
    df.to_csv(filename, index=False)

if __name__ == "__main__":
    throughput_data = fetch_data(throughput_query)
    save_to_csv(throughput_data, 'throughput_analysis.csv')
    
    latency_data = fetch_data(latency_query)
    save_to_csv(latency_data, 'latency_analysis.csv')
    
    cursor.close()
    conn.close()
