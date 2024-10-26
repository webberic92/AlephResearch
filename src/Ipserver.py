from http.server import BaseHTTPRequestHandler, HTTPServer
import json

# Define the network range and starting IP
network_base = "192.168.0."
start_ip = 2
max_ip = 254
assigned_ips = []  # To track assigned IPs

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
        else:
            self.send_response(404)
            self.end_headers()

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
    print(f'Starting IP allocation server on port {port}...')
    httpd.serve_forever()

if __name__ == "__main__":
    run()
