from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import sys
from threading import Lock

# List to track assigned IPs
# Keeps track of the IP addresses of nodes that have registered as ready.
assigned_ips = []

# Dictionary to track node readiness state
# Maps node IPs to their readiness status (e.g., True if the node is ready).
node_status = {}

# Global transaction state
# Keeps track of the overall state of the network.
global_state = {
    "current_round_id": 1,  # The current round (round) of the transaction process.
    "current_node_id": 1,   # The ID of the node that is expected to submit the next transaction.
    "total_nodes": 0        # Total number of nodes in the network (set at runtime).
}

# Lock for thread-safe operations
# Ensures that multiple threads don't modify shared data structures simultaneously.
lock = Lock()

class IPAllocationHandler(BaseHTTPRequestHandler):
    """
    HTTP server handler to manage node readiness, transaction turns, and other node interactions.
    """

    def do_GET(self):
        """
        Handles GET requests.
        Endpoints include:
        - `/check_all_ready`: Checks if all nodes are registered and ready.
        - `/get_all_nodes`: Returns the list of all registered nodes.
        - `/is_turn`: Checks if it's the turn of a specific node for the current round.
        """
        if self.path == "/check_all_ready":
            with lock:
                # Checks if the number of ready nodes matches the total nodes expected.
                all_ready = len(assigned_ips) == global_state["total_nodes"]
            self._send_response(200, {"all_ready": all_ready})

        elif self.path == "/get_all_nodes":
            with lock:
                # Returns the list of registered node IPs.
                self._send_response(200, {"node_ips": assigned_ips})

        elif self.path.startswith("/is_turn"):
            # Parse query parameters (node_id and round_id).
            query = self.path.split("?")[-1]
            params = dict(qc.split("=") for qc in query.split("&"))
            try:
                # Extract and validate node_id and round_id from the query parameters.
                node_id = int(params.get("node_id", -1))
                round_id = int(params.get("round_id", -1))

                if node_id == -1 or round_id == -1:
                    # Missing or invalid parameters.
                    self._send_response(
                        400, 
                        {"error": "Missing or invalid parameters. 'node_id' and 'round_id' must be provided as integers."}
                    )
                    return

                with lock:
                    # Check if it's the specified node's turn for the current round.
                    is_turn = (
                        round_id == global_state["current_round_id"] and
                        node_id == global_state["current_node_id"]
                    )

                if is_turn:
                    # Node's turn confirmed.
                    self._send_response(200, {"is_turn": True})
                else:
                    # Provide detailed feedback if the check fails.
                    self._send_response(
                        403,
                        {
                            "is_turn": False,
                            "error": "Not your turn.",
                            "expected_node_id": global_state["current_node_id"],
                            "expected_round_id": global_state["current_round_id"],
                            "received_node_id": node_id,
                            "received_round_id": round_id,
                        }
                    )
            except ValueError as e:
                # Log and respond to invalid data parsing issues.
                self._send_response(
                    400, 
                    {"error": f"Invalid query parameters. Details: {str(e)}"}
                )
        else:
            # Handle invalid endpoints.
            self._send_response(404, {"error": "Endpoint not found"})


    def do_POST(self):
        """
        Handles POST requests.
        Endpoints include:
        - `/node_ready`: Marks a node as ready.
        - `/submit_transaction`: Submits a transaction for the current round.
        """
        if self.path == "/node_ready":
            self._handle_node_ready()
        elif self.path == "/submit_transaction":
            self._handle_submit_transaction()
        else:
            # Handle invalid endpoints.
            self._send_response(404, {"error": "Endpoint not found"})

    def _handle_node_ready(self):
        """
        Handles node readiness registration.
        - Registers the node's IP as "ready".
        """
        content_length = int(self.headers['Content-Length'])
        post_data = self.rfile.read(content_length)
        try:
            data = json.loads(post_data)
            node_ip = data.get("node_ip")
            if node_ip:
                with lock:
                    # Add the node's IP to the list if it's not already registered.
                    if node_ip not in assigned_ips:
                        assigned_ips.append(node_ip)
                        assigned_ips.sort()  # Keep the list sorted for consistency.
                    # Mark the node as ready in the `node_status` dictionary.
                    node_status[node_ip] = True
                self._send_response(200, {"status": "Node IP registered as ready"})
            else:
                # Error if no `node_ip` is provided in the request.
                self._send_response(400, {"error": "No node_ip provided"})
        except json.JSONDecodeError:
            # Error if the request body is not valid JSON.
            self._send_response(400, {"error": "Invalid JSON"})

    def _handle_submit_transaction(self):
        """
        Handles transaction submission by nodes.
        - Advances to the next node in sequence.
        - When all nodes have submitted, increments the round and restarts with node 1.
        """
        content_length = int(self.headers['Content-Length'])
        post_data = self.rfile.read(content_length)

        try:
            data = json.loads(post_data)
            node_id = data.get("node_id")

            with lock:
                if global_state["current_node_id"] == node_id:
                    # ✅ If last node submits, reset to Node 1 and increment round
                    if global_state["current_node_id"] == global_state["total_nodes"]:
                        global_state["current_node_id"] = 1
                        global_state["current_round_id"] += 1  # 🔥 Corrected round increment
                        print(f"✅ round incremented to {global_state['current_round_id']}. Restarting node sequence.")

                    else:
                        # Otherwise, just move to the next node
                        global_state["current_node_id"] += 1

                    self._send_response(200, {
                        "status": "Transaction submitted successfully",
                        "current_round_id": global_state["current_round_id"],  # ✅ Return correct round
                        "next_node_id": global_state["current_node_id"]
                    })
                else:
                    # ❌ Reject if it's not the submitting node's turn
                    self._send_response(403, {
                        "error": "Not your turn",
                        "expected_node_id": global_state["current_node_id"],
                        "expected_round_id": global_state["current_round_id"],
                        "received_node_id": node_id,
                        "received_round_id": global_state["current_round_id"]
                    })

        except json.JSONDecodeError:
            # ❌ Handle invalid JSON request
            self._send_response(400, {"error": "Invalid JSON"})



    def _send_response(self, status_code, response):
        """
        Helper function to send JSON responses.
        - `status_code`: The HTTP status code to send.
        - `response`: The JSON response body to send.
        """
        self.send_response(status_code)
        self.send_header('Content-type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(response).encode())

def run(server_class=HTTPServer, handler_class=IPAllocationHandler, port=8080):
    """
    Starts the HTTP server.
    - `port`: The port on which the server should listen.
    """
    server_address = ('', port)
    httpd = server_class(server_address, handler_class)
    print(f"Starting IP readiness server on port {port} with total nodes required: {global_state['total_nodes']}...")
    httpd.serve_forever()

if __name__ == "__main__":
    # Ensure the script is called with the required number of nodes.
    if len(sys.argv) < 2:
        print("Usage: python3 IpServer.py <INSTANCES_NUMBER>")
        sys.exit(1)
    # Set the total number of nodes in the global state.
    global_state["total_nodes"] = int(sys.argv[1])
    # Start the server on port 8080.
    run()
