import json
import os
import matplotlib.pyplot as plt
import networkx as nx
from pathlib import Path

# Define path where finalized DAG files are stored
base_path = "/home/aleph-node/finalized_dag"

# Collect all round JSON files
dag_files = sorted(Path(base_path).glob("round*.json"), key=lambda x: int(x.stem.replace("round", "")))

# Initialize a directed graph
G = nx.DiGraph()

# Load units from each file and add edges based on parent relations
for file in dag_files:
    with open(file, "r") as f:
        units = json.load(f)
        for unit in units:
            unit_id = unit["unit_id"]
            G.add_node(unit_id, round=unit["round"])
            for parent_id in unit["parent_hashes"]:
                G.add_edge(parent_id, unit_id)

# Layout and draw DAG
plt.figure(figsize=(16, 10))
pos = nx.spring_layout(G, k=0.8, iterations=100)
nx.draw(G, pos, with_labels=True, node_size=1000, node_color="skyblue", font_size=10, arrows=True)
plt.title("DAG Visualization Across Rounds")
plt.axis("off")
plt.show()
