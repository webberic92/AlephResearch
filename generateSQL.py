import os
import re

BASE_PATH = "logs/.final"
TABLE_SUMMARY = "experiment_summary"
TABLE_TPS = "node_tps"
TABLE_OVERHEAD = "node_overhead"
TABLE_RESOURCE = "node_resource_utilization"

summary_id = 1
sql_statements = []

def parse_filename(fname):
    m = re.search(r"(\d+)nodes.*?(\d+)?txpb.*?(\d+)?rounds", fname)
    if m:
        nodes, txpb, rounds = m.groups()
        return int(nodes), int(txpb or 0), int(rounds or 0)
    return None, None, None

def sanitize(val):
    return val.replace("'", "''")

def parse_file(path, protocol, fname):
    global summary_id
    with open(path) as f:
        content = f.read()

    nodes, txpb, rounds = parse_filename(fname)
    if not nodes:
        return

    avg_tps = re.search(r"Average TPS.*?: ([\d.]+)", content)
    avg_overhead = re.search(r"Average Communication Overhead.*?: ([\d.]+)", content)
    avg_cpu = re.search(r"Average CPU Utilization.*?: ([\d.]+)", content)
    avg_mem = re.search(r"Average Memory Utilization.*?: ([\d.]+)", content)
    latency = re.search(r"Total Latency: ([\d.]+)", content)

    sql_statements.append(f"""INSERT INTO {TABLE_SUMMARY} 
(protocol, filename, nodes, txpb, rounds, avg_tps, avg_overhead, avg_cpu, avg_mem, latency_seconds)
VALUES ('{protocol}', '{sanitize(fname)}', {nodes}, {txpb}, {rounds}, 
{float(avg_tps.group(1)) if avg_tps else 'NULL'}, 
{float(avg_overhead.group(1)) if avg_overhead else 'NULL'},
{float(avg_cpu.group(1)) if avg_cpu else 'NULL'},
{float(avg_mem.group(1)) if avg_mem else 'NULL'},
{float(latency.group(1)) if latency else 'NULL'}
);""")

    tps = re.findall(r"node-(\d+).*?TPS: ([\d.]+)", content)
    for node, val in tps:
        sql_statements.append(f"INSERT INTO {TABLE_TPS} (experiment_id, node_id, tps) VALUES ({summary_id}, 'node-{node}', {val});")

    overheads = re.findall(r"node-(\d+)/\s+Overhead: (\d+)", content)
    for node, val in overheads:
        sql_statements.append(f"INSERT INTO {TABLE_OVERHEAD} (experiment_id, node_id, overhead_messages) VALUES ({summary_id}, 'node-{node}', {val});")

    resources = re.findall(r"node-(\d+)/\s+([\d.]+)\s+([\d.]+)", content)
    for node, cpu, mem in resources:
        sql_statements.append(f"INSERT INTO {TABLE_RESOURCE} (experiment_id, node_id, cpu_util, mem_util) VALUES ({summary_id}, 'node-{node}', {cpu}, {mem});")

    summary_id += 1

def main():
    for proto in ['merkle', 'rsa']:
        full_path = os.path.join(BASE_PATH, proto)
        if not os.path.exists(full_path):
            continue
        for fname in os.listdir(full_path):
            if not fname.endswith('.txt'):
                continue
            parse_file(os.path.join(full_path, fname), proto.upper(), fname)

    with open("insert_all.sql", "w") as out:
        out.write("\n".join(sql_statements))

if __name__ == "__main__":
    main()
