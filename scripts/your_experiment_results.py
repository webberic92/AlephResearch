# save this as init_db.py
import sqlite3

# Connect to (or create) the database
conn = sqlite3.connect("your_experiment_results.db")
cursor = conn.cursor()

# Create the table (adjust columns to match your data types)
cursor.execute("""
CREATE TABLE IF NOT EXISTS experiment_summary (
    id INTEGER PRIMARY KEY,
    protocol TEXT,
    filename TEXT,
    nodes INTEGER,
    txpb INTEGER,
    rounds INTEGER,
    avg_tps REAL,
    avg_overhead REAL,
    avg_cpu REAL,
    avg_mem REAL,
    latency_seconds REAL
);
""")

# Insert your raw SQL data here
rows = [
    (3,"MERKLE","16nodes_256txpb_3rounds_MERK.txt",16,256,3,256,195,38.57,22.93,36.258),
    (4,"MERKLE","32nodes_256txpb_3rounds_MERK.txt",32,256,3,256,395,38.34,28.56,185.994),
    (5,"MERKLE","64nodes_25txpb_25rounds_MERK.txt",64,25,25,25,5523.42,21.68,13.85,776.14),
    (2,"MERKLE","10nodes_25txpb_2rounds_MERK.txt",10,25,2,25,150.1,39.99,27.72,7.002),
    (1,"MERKLE","104nodes_5txpb_4rounds_MERK.txt",104,5,4,5,1638.69,11.6,12.16,86.979),
    (8,"RSA","16nodes_256txpb_3rounds_RSA.txt",16,256,3,256,290.87,34.71,13.79,1131.509),
    (7,"RSA","10nodes_25txpb_3rounds_RSA.txt",10,25,3,25,171,62.06,27.71,25.987),
    (6,"RSA","104nodes_5txpb_4rounds_RSA.txt",104,5,4,5,1908.59,16.52,13.99,428.528)
]

cursor.executemany("""
INSERT INTO experiment_summary 
(id, protocol, filename, nodes, txpb, rounds, avg_tps, avg_overhead, avg_cpu, avg_mem, latency_seconds)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
""", rows)

# Save and close
conn.commit()
conn.close()

print("Database populated successfully.")
