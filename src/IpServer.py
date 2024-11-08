from http.server import BaseHTTPRequestHandler, HTTPServer
import json
from threading import Lock

# List to track assigned IPs
assigned_ips = []  # Stores the IPs of ready nodes

# Dictionary to track node readiness
node_status = {}
total_nodes = 4  # Define the total number of nodes required for readiness

# Lock for thread-safe access
lock = Lock()

class IPAllocationHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/check_all_ready":
            # Check if all nodes are ready
            with lock:
                all_ready = len(assigned_ips) == total_nodes
            response = {"all_ready": all_ready}
            self.send_response(200)
            self.send_header('Content-type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps(response).encode())
        
        elif self.path == "/get_all_nodes":
            # Return the list of all registered node IPs
            with lock:
                response = {"node_ips": assigned_ips}
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
                node_ip = data.get("node_ip")
                if node_ip:
                    with lock:
                        # Add the IP to the assigned_ips list if not already present
                        if node_ip not in assigned_ips:
                            assigned_ips.append(node_ip)
                        node_status[node_ip] = True  # Mark the node as ready
                    response = {"status": "Node IP registered as ready"}
                    self.send_response(200)
                else:
                    response = {"error": "No node_ip provided"}
                    self.send_response(400)
            except json.JSONDecodeError:
                response = {"error": "Invalid JSON"}
                self.send_response(400)
            # Send response
            self.send_header('Content-type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps(response).encode())

def run(server_class=HTTPServer, handler_class=IPAllocationHandler, port=8080):
    server_address = ('', port)
    httpd = server_class(server_address, handler_class)
    print(f'Starting IP readiness server on port {port}...')
    httpd.serve_forever()

if __name__ == "__main__":
    run()
