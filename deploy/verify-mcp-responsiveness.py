#!/usr/bin/env python3
"""Prove a partial MCP request cannot block an independent API request."""

import argparse
import json
import os
import pathlib
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request


TOKEN = "mcp-responsiveness-test-token"
PORT = 38141
STATUS_URL = f"http://127.0.0.1:{PORT}/api/v1/status"


def status(timeout: float) -> int:
    request = urllib.request.Request(
        STATUS_URL, headers={"Authorization": f"Bearer {TOKEN}"}
    )
    with urllib.request.urlopen(request, timeout=timeout) as response:
        response.read()
        return response.status


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", type=pathlib.Path)
    parser.add_argument("--expect", choices=("blocked", "responsive"), required=True)
    args = parser.parse_args()

    with tempfile.TemporaryDirectory(prefix="mcp-responsiveness-") as directory:
        environment = os.environ.copy()
        environment.update(
            {
                "POCKETSMITH_DB": f"{directory}/pocketsmith.db",
                "REPORTING_API_TOKEN": TOKEN,
                "SERVE_PORT": str(PORT),
            }
        )
        server = subprocess.Popen(
            [str(args.binary.resolve()), "serve"],
            env=environment,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        slow = None
        try:
            for _ in range(100):
                try:
                    if status(0.2) == 200:
                        break
                except (OSError, urllib.error.URLError):
                    time.sleep(0.05)
            else:
                raise AssertionError("server did not become ready")

            slow = socket.create_connection(("127.0.0.1", PORT), timeout=1)
            slow.sendall(
                b"POST /api/v1/mcp HTTP/1.1\r\n"
                b"Host: 127.0.0.1\r\n"
                b"Authorization: Bearer mcp-responsiveness-test-token\r\n"
                b"Content-Type: application/json\r\n"
                b"Content-Length: 65536\r\n\r\n"
                b'{"jsonrpc":"2.0"'
            )
            time.sleep(0.1)

            started = time.monotonic()
            try:
                code = status(0.75)
                elapsed = time.monotonic() - started
                responsive = code == 200 and elapsed < 0.5
            except (OSError, urllib.error.URLError):
                elapsed = time.monotonic() - started
                code = None
                responsive = False

            print(json.dumps({"expect": args.expect, "status": code, "seconds": elapsed}))
            if args.expect == "responsive":
                assert responsive, "independent request was blocked by the partial MCP body"
            else:
                assert not responsive, "base server unexpectedly remained responsive"
        finally:
            if slow is not None:
                slow.close()
            server.terminate()
            try:
                server.wait(timeout=2)
            except subprocess.TimeoutExpired:
                server.kill()
            if server.returncode not in (None, 0, -15):
                print(server.stderr.read())


if __name__ == "__main__":
    main()
