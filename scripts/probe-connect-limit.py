#!/usr/bin/env python3
"""Opens N WebSocket handshakes to a relay's /v1/sync and reports how many the relay refused
before Hello (connect_ip limit, ADR 0020). Stdlib only, so it runs anywhere.
  scripts/probe-connect-limit.py https://relay.tryzoen.com 70 [--spoof 203.0.113.9]
A refused handshake gets an immediate binary frame (ErrorCode::RateLimited); an admitted one
gets nothing until the client says Hello."""
import base64, os, socket, ssl, sys, urllib.parse
from concurrent.futures import ThreadPoolExecutor

def handshake(url, spoof=None):
    u = urllib.parse.urlparse(url)
    port = u.port or (443 if u.scheme == "https" else 80)
    raw = socket.create_connection((u.hostname, port), timeout=10)
    s = ssl.create_default_context().wrap_socket(raw, server_hostname=u.hostname) if u.scheme == "https" else raw
    key = base64.b64encode(os.urandom(16)).decode()
    extra = f"Fly-Client-IP: {spoof}\r\nCF-Connecting-IP: {spoof}\r\n" if spoof else ""
    s.sendall((f"GET /v1/sync HTTP/1.1\r\nHost: {u.hostname}\r\nUpgrade: websocket\r\n"
               f"Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n{extra}\r\n").encode())
    head = b""
    while b"\r\n\r\n" not in head:
        chunk = s.recv(4096)
        if not chunk:
            break
        head += chunk
    status = head.split(b"\r\n", 1)[0].decode(errors="replace")
    rest = head.split(b"\r\n\r\n", 1)[1] if b"\r\n\r\n" in head else b""
    s.settimeout(1.5)
    try:
        if not rest:
            rest = s.recv(4096)
        refused = bool(rest)
    except socket.timeout:
        refused = False
    s.close()
    return status, refused

def main():
    url, n = sys.argv[1], int(sys.argv[2])
    spoof = sys.argv[sys.argv.index("--spoof") + 1] if "--spoof" in sys.argv else None
    # All at once: the bucket refills 1/s, so a slow sequential probe would never drain it.
    with ThreadPoolExecutor(max_workers=n) as pool:
        results = list(pool.map(lambda _: handshake(url, spoof), range(n)))
    for status, _ in results:
        if not status.startswith("HTTP/1.1 101"):
            print(status)
    refused = sum(r for _, r in results)
    print(f"{n} concurrent handshakes{' (spoofing ' + spoof + ')' if spoof else ''}: "
          f"{n - refused} admitted, {refused} refused")

main()
