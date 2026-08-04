"""o7b1 — deterministic B1 context vertical for the case-0001 golden fixture.

This package is NOT part of the cargo workspace and MUST NOT be imported by any
production 007 crate. It is read-only with respect to its inputs (RAW blobs in
the owner's external local CAS): it never writes to the source CAS, never needs
network access, and never needs secrets.

Everything here is built to be byte-for-byte reproducible: the same inputs
produce byte-identical canonical outputs. Wall-clock time appears only in
non-canonical run receipts, never in a digest-bound artifact.
"""

TOOLING_VERSION = "0"
