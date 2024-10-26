from http.server import BaseHTTPRequestHandler, HTTPServer
import json
from threading import Lock

# Define the network range and starting IP
network_base = "192.168.0."
start_ip = 2
max_ip = 254
assigned_ips = []  # To track assigned IPs

# Dictionary to track node readiness
node_status = {}
total_nodes = 4  # Define the total number of nodes required for readiness

# Lock for thread-safe access
lock = Lock()

class IPAllocationHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/get_ip":
            # Allocate the next available IP
            next_ip = self.get_next_ip()
            if next_ip:
                response = {"ip": next_ip}
                self.send_response(200)
                self.send_header('Content-type', 'application/json')
                self.end_headers()
                self.wfile.write(json.dumps(response).encode())
            else:
                # No more available IPs
                self.send_response(503)
                self.end_headers()
        
        elif self.path == "/check_all_ready":
            # Check if all nodes are ready
            with lock:
                all_ready = len(node_status) >= total_nodes and all(node_status.values())
            response = {"all_ready": all_ready}
            self.send_response(200)
            self.send_header('Content-type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps(response).encode())

        else:
            self.send_response(404)
            self.end_headers()

    def do_POST(self):
        if self.path == "/node_ready":
            # Handle node readiness registration
            content_length = int(self.headers['Content-Length'])
            post_data = self.rfile.read(content_length)
            try:
                data = json.loads(post_data)
                node_id = data.get("node_id")
                if node_id:
                    with lock:
                        node_status[node_id] = True  # Mark the node as ready
                    response = {"status": "Node registered as ready"}
                    self.send_response(200)
                else:
                    response = {"error": "No node_id provided"}
                    self.send_response(400)
            except json.JSONDecodeError:
                response = {"error": "Invalid JSON"}
                self.send_response(400)
            # Send response
            self.send_header('Content-type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps(response).encode())

    def get_next_ip(self):
        global start_ip
        for i in range(start_ip, max_ip + 1):
            ip_address = network_base + str(i)
            if ip_address not in assigned_ips:
                assigned_ips.append(ip_address)
                return ip_address
        return None

def run(server_class=HTTPServer, handler_class=IPAllocationHandler, port=8080):
    server_address = ('', port)
    httpd = server_class(server_address, handler_class)
    print(f'Starting IP allocation and readiness server on port {port}...')
    httpd.serve_forever()

if __name__ == "__main__":
    run()
