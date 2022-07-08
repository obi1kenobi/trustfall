#!/usr/bin/env python
from http import server # Python 3

class MyHTTPRequestHandler(server.SimpleHTTPRequestHandler):
    def end_headers(self) -> None:
        self.send_custom_headers()
        super().end_headers()

    def send_custom_headers(self) -> None:
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")

    def do_GET(self) -> None:
        return super().do_GET()

if __name__ == '__main__':
    server.test(HandlerClass=MyHTTPRequestHandler)
