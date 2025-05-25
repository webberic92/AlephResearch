# Load necessary libraries
import sqlite3
import pandas as pd
import seaborn as sns
import matplotlib.pyplot as plt

# Connect to SQLite database (replace with your actual DB path)
conn = sqlite3.connect("your_experiment_results.db")  # or use .db file if SQLite
query = """
SELECT *
FROM experiment_summary
"""

# Load SQL data into a pandas DataFrame
df = pd.read_sql_query(query, conn)

# Preprocess: convert relevant columns to numeric (in case they're stored as strings)
df['avg_tps'] = pd.to_numeric(df['avg_tps'], errors='coerce')
df['txpb'] = pd.to_numeric(df['txpb'], errors='coerce')
df['nodes'] = pd.to_numeric(df['nodes'], errors='coerce')

# Plot: TPS vs Batch Size (log-log), color by nodes, marker by protocol
plt.figure(figsize=(10, 6))
sns.scatterplot(
    data=df,
    x='txpb',
    y='avg_tps',
    hue='nodes',
    style='protocol',
    palette='viridis',
    markers=True,
    s=150
)

plt.xscale('log')
plt.yscale('log')
plt.xlabel("Batch Size (log)")
plt.ylabel("TPS (log)")
plt.title("TPS vs Batch Size (log-log scale)")
plt.grid(True, which="both", linestyle="--", linewidth=0.5)
plt.tight_layout()
plt.show()
