from http.server import BaseHTTPRequestHandler, HTTPServer
import os
from datetime import datetime
from simServerDepend import ServerConfig
import json
from urllib.parse import urlparse, parse_qs

class RequestHandler(BaseHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        self.config = ServerConfig()
        super().__init__(*args, **kwargs)

    def log_request_to_file(self, status=200):
        # Create data directory if it doesn't exist
        os.makedirs('./data', exist_ok=True)
        
        # Log request details
        timestamp = datetime.now().strftime('%Y-%m-%d %H:%M:%S')
        log_entry = self.config.log_format.format(
            timestamp=timestamp,
            ip=self.client_address[0],
            method=self.command,
            path=self.path,
            status=status
        ) + "\n"
        
        with open(self.config.log_file, 'a') as f:
            f.write(log_entry)

    def send_json_response(self, data, status=200):
        self.send_response(status)
        self.send_header('Content-type', 'application/json')
        self.end_headers()
        self.wfile.write(json.dumps(data).encode())

    def do_GET(self):
        parsed_path = urlparse(self.path)
        path = parsed_path.path
        query = parse_qs(parsed_path.query)
        
        if path == '/test':
            # 测试端点，返回请求信息
            response_data = {
                'status': 'success',
                'query': query,
                'version': self.config.version,
                'timestamp': datetime.now().isoformat()
            }
            self.send_json_response(response_data)
        elif path == '/health':
            # 健康检查端点
            self.send_json_response({'status': 'healthy'})
        else:
            # 默认HTML响应
            self.send_response(200)
            self.send_header('Content-type', 'text/html')
            self.end_headers()
            
            html_content = f"""
            <html>
                <head>
                    <title>Simple Server v{self.config.version}</title>
                </head>
                <body>
                    <h1>{self.config.welcome_message}</h1>
                    <p>This is a simple HTTP server (v{self.config.version}).</p>
                    <p>Server Time: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}</p>
                </body>
            </html>
            """
            self.wfile.write(html_content.encode())
        
        self.log_request_to_file()

def run_server():
    config = ServerConfig()
    server_address = ('', config.port)
    httpd = HTTPServer(server_address, RequestHandler)
    print(f"Server running on port {config.port}...")
    httpd.serve_forever()

if __name__ == "__main__":
    run_server()
