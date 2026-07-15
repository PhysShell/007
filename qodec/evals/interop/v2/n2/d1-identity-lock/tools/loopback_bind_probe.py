#!/usr/bin/env python3
"""N2-D1b: positive loopback bind+connect probe, run THROUGH the exact same
outer network namespace + Sandboy confinement (network_enforcement_mode =
"outer-netns-loopback-only") a real jvm-gradle capture uses. Proves the
narrow escape hatch genuinely permits an OS-chosen ephemeral loopback port
-- the specific capability Gradle's own daemon architecture needs and
Landlock's fixed-port-list tcp_bind cannot express -- while
canary/tools/network_probe.py (run alongside this, same envelope) proves
real external connectivity is still fully blocked by the outer netns.
Binds to 127.0.0.1:0 (OS-chosen port), listens, connects to itself, and
exchanges one probe message -- exactly the shape of a client<->daemon
loopback handshake, never any repository-controlled destination.
"""
import json
import socket
import sys


def main():
    try:
        server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        server.bind(("127.0.0.1", 0))
        server.listen(1)
        port = server.getsockname()[1]

        client = socket.create_connection(("127.0.0.1", port), timeout=2.0)
        conn, _ = server.accept()
        client.sendall(b"n2d1b-loopback-probe\n")
        received = conn.recv(64)
        client.close()
        conn.close()
        server.close()

        result = {
            "result": "ALLOWED",
            "bound_port": port,
            "echoed_correctly": received == b"n2d1b-loopback-probe\n",
        }
        print(json.dumps(result))
        sys.exit(0 if result["echoed_correctly"] else 1)
    except OSError as e:
        print(json.dumps({"result": "DENIED", "errno": e.errno, "error": str(e)}))
        sys.exit(1)


if __name__ == "__main__":
    main()
