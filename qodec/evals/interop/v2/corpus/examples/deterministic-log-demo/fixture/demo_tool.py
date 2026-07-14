#!/usr/bin/env python3
"""First-party deterministic build-log generator (corpus plumbing demo).

NON-BENCHMARK. Emits a fixed, deterministic build-log-like stream to stdout with
repeated lines, one warning and one error-like line, then exits with a fixed
non-zero code. No timestamps, no randomness, no network — byte-identical on every
run on x86_64-linux.
"""
import sys

LINES = [
    "[build] compiling module core v0.1.0",
    "[build] compiling module core v0.1.0",
    "[build] compiling module core v0.1.0",
    "[build] linking target/demo",
    "warning: unused variable `tmp` at src/core/parse.rs:42",
    "error[E0308]: mismatched types at src/core/parse.rs:120",
    "[build] finished: 1 failed check, 1 flagged item",
]


def main() -> int:
    sys.stdout.write("\n".join(LINES) + "\n")
    sys.stdout.flush()
    return 3


if __name__ == "__main__":
    sys.exit(main())
