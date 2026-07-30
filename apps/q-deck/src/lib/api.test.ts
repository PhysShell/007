// The regression this file exists to prove (found on independent re-gate of
// Q-Deck R0.6, docs/q-deck/r06-verdict-fidelity.md): schema_version isn't
// just a version label — it's the one signal that separates "this client
// understands every value in this response" from "it doesn't." A closed
// client-side union like RunStatus, used for control flow (isSealedRun),
// cannot safely treat an unrecognized value as harmless the way a display-only
// field like event_type can. checkSchema must reject a mismatched version
// BEFORE any status logic ever sees the body — not just for an arbitrary
// unknown future version, but specifically for schema_version 1, the real
// pre-R0.6 value an old, not-yet-upgraded build would still expect.

import { describe, expect, it } from "vitest";
import { checkSchema, EXPECTED_SCHEMA_VERSION, UnsupportedSchemaError } from "./api";
import { isSealedRun } from "./types";

describe("checkSchema", () => {
  it("accepts the current schema_version", () => {
    expect(() => checkSchema({ schema_version: EXPECTED_SCHEMA_VERSION })).not.toThrow();
  });

  it("rejects schema_version 1 — the real pre-R0.6 value, not just an arbitrary unknown one", () => {
    // 1 is not a made-up "future version this build has never seen": it is
    // the actual API_SCHEMA_VERSION every Q-Deck build understood before
    // this vertical. A server that regressed to it, or a proxy/cache
    // serving a stale pre-bump response, must be rejected the same way any
    // other mismatch is — never treated as "close enough."
    expect(() => checkSchema({ schema_version: 1 })).toThrow(UnsupportedSchemaError);
  });

  it("rejecting happens before any RunStatus is ever read — a run body carrying a status this build's isSealedRun would call sealed does not sneak through under the wrong version", () => {
    const staleServerBody = {
      schema_version: 1,
      run_id: "run-1",
      conversation_id: "conv-1",
      parent_run_id: null,
      agent: "claude",
      role: "implementer",
      status: "blocked" as const,
      created_at: 0,
      finished_at: 0,
    };

    // Confidence check on the premise: if the version gate were skipped or
    // buggy, this body's own status WOULD read as sealed — proving the
    // version check is the only thing standing between an old client and
    // silently accepting it.
    expect(isSealedRun(staleServerBody.status)).toBe(true);

    expect(() => checkSchema(staleServerBody)).toThrow(UnsupportedSchemaError);
  });
});
