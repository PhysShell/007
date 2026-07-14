#!/usr/bin/env python3
"""Connectivity probe run THROUGH run_confined_build.sh (i.e. inside the same
outer network namespace + Sandboy confinement the real build uses) to produce
concrete evidence for network-isolation-report.json. Attempts a handful of
outbound connections; every one is expected to fail from inside the isolated
namespace. Never contacts anything but well-known, documentation-safe public
endpoints (no repository-controlled destination)."""
import json
import socket
import sys

TARGETS = [
    ("1.1.1.1", 443, "tcp"),
    ("8.8.8.8", 53, "udp"),
    ("api.nuget.org", 443, "tcp-dns-plus-connect"),
]


def attempt(host, port, kind):
    try:
        if kind == "udp":
            s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            s.settimeout(2.0)
            s.sendto(b"probe", (host, port))
        else:
            s = socket.create_connection((host, port), timeout=2.0)
        s.close()
        return {"host": host, "port": port, "kind": kind, "result": "REACHABLE"}
    except Exception as e:  # noqa: BLE001 - report exactly what happened, any exception is a pass here
        return {"host": host, "port": port, "kind": kind, "result": "UNREACHABLE", "error": f"{type(e).__name__}: {e}"}


def main():
    results = [attempt(h, p, k) for h, p, k in TARGETS]
    print(json.dumps({"probe_results": results}))
    any_reachable = any(r["result"] == "REACHABLE" for r in results)
    sys.exit(1 if any_reachable else 0)


if __name__ == "__main__":
    main()
