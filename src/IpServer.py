from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import sys
from threading import Lock

# List to track assigned IPs
assigned_ips = []  # Stores the IPs of ready nodes

# Dictionary to track node readiness
node_status = {}

# Global transaction state
global_state = {
    "current_epoch_id": 1,
    "current_node_id": 1,  # Node expected to submit next
    "total_nodes": 0  # To be set at runtime
}

# Lock for thread-safe access
lock = Lock()

class IPAllocationHandler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/check_all_ready":
            with lock:
                all_ready = len(assigned_ips) == global_state["total_nodes"]
            self._send_response(200, {"all_ready": all_ready})
        elif self.path == "/get_all_nodes":
            with lock:
                self._send_response(200, {"node_ips": assigned_ips})
        elif self.path == "/validate_nodes":
            with lock:
                self._send_response(200, {"all_nodes": assigned_ips})
        elif self.path.startswith("/is_turn"):
            query = self.path.split("?")[-1]
            params = dict(qc.split("=") for qc in query.split("&"))
            node_id = int(params.get("node_id", -1))
            epoch_id = int(params.get("epoch_id", -1))
            with lock:
                is_turn = (epoch_id == global_state["current_epoch_id"] and
                           node_id == global_state["current_node_id"])
                self._send_response(200 if is_turn else 403, {"is_turn": is_turn})
        else:
            self._send_response(404, {})

    def do_POST(self):
        if self.path == "/node_ready":
            self._handle_node_ready()
        elif self.path == "/submit_transaction":
            self._handle_submit_transaction()
        else:
            self._send_response(404, {})

    def _handle_node_ready(self):
        content_length = int(self.headers['Content-Length'])
        post_data = self.rfile.read(content_length)
        try:
            data = json.loads(post_data)
            node_ip = data.get("node_ip")
            if node_ip:
                with lock:
                    if node_ip not in assigned_ips:
                        assigned_ips.append(node_ip)
                        assigned_ips.sort()
                    node_status[node_ip] = True
                self._send_response(200, {"status": "Node IP registered as ready"})
            else:
                self._send_response(400, {"error": "No node_ip provided"})
        except json.JSONDecodeError:
            self._send_response(400, {"error": "Invalid JSON"})

    def _handle_submit_transaction(self):
        content_length = int(self.headers['Content-Length'])
        post_data = self.rfile.read(content_length)
        try:
            data = json.loads(post_data)
            node_id = data.get("node_id")
            with lock:
                if global_state["current_node_id"] == node_id:
                    global_state["current_node_id"] = (global_state["current_node_id"] % global_state["total_nodes"]) + 1
                    self._send_response(200, {"status": "Transaction submitted successfully"})
                else:
                    self._send_response(403, {"error": "Not your turn"})
        except json.JSONDecodeError:
            self._send_response(400, {"error": "Invalid JSON"})

    def _send_response(self, status_code, response):
        self.send_response(status_code)
        self.send_header('Content-type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(response).encode())

def run(server_class=HTTPServer, handler_class=IPAllocationHandler, port=8080):
    server_address = ('', port)
    httpd = server_class(server_address, handler_class)
    print(f'Starting IP readiness server on port {port} with total nodes required: {global_state["total_nodes"]}...')
    httpd.serve_forever()

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python3 IpServer.py <INSTANCES_NUMBER>")
        sys.exit(1)
    global_state["total_nodes"] = int(sys.argv[1])
    run()
