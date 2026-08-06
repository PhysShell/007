// Q-Deck A0.5 (docs/q-deck/a0-candidate-state.md §9): CandidateStateCard's
// own contract — exhaustive over the four real materialization_status
// values, safe on an unrecognized fifth one, safe when candidate-state data
// is entirely absent, and never conflates candidate-state provenance
// (candidate_source_run_id) with run lineage (parent_run_id, which this
// component never even reads — that's RunPage's own concern).

import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import CandidateStateCard from "./CandidateStateCard.svelte";
import type { RunDto } from "../lib/types";

function baseRun(overrides: Partial<RunDto> = {}): RunDto {
  return {
    schema_version: 2,
    run_id: "run-current",
    conversation_id: "conv-1",
    parent_run_id: null,
    agent: "claude",
    role: "implementer",
    status: "completed",
    created_at: 1_700_000_000_000,
    finished_at: 1_700_000_005_000,
    ...overrides,
  };
}

describe("CandidateStateCard", () => {
  it.each([
    ["materialized", "materialized"],
    ["not_applicable", "not applicable"],
    ["failed", "failed"],
    ["verification_failed", "verification failed"],
  ] as const)("renders the %s status as %j", (status, expectedLabel) => {
    render(CandidateStateCard, { props: { run: baseRun({ materialization_status: status }) } });
    expect(screen.getByText(expectedLabel)).toBeInTheDocument();
  });

  it("renders an unrecognized future status verbatim instead of hiding or misclassifying it", () => {
    render(CandidateStateCard, {
      props: { run: baseRun({ materialization_status: "reconciling" }) },
    });
    expect(screen.getByText("reconciling")).toBeInTheDocument();
  });

  it("renders a clear unavailable state, without error, when materialization_status is entirely absent", () => {
    const run = baseRun();
    delete run.materialization_status;
    expect(() => render(CandidateStateCard, { props: { run } })).not.toThrow();
    expect(screen.getByText("unavailable")).toBeInTheDocument();
    expect(screen.getByText("No candidate-state data for this run.")).toBeInTheDocument();
  });

  it("renders a clear unavailable state, without error, when all three candidate fields are null", () => {
    const run = baseRun({
      candidate_source_run_id: null,
      candidate_tree_oid: null,
      materialization_status: null,
    });
    expect(() => render(CandidateStateCard, { props: { run } })).not.toThrow();
    expect(screen.getByText("unavailable")).toBeInTheDocument();
    expect(screen.getByText("No candidate-state data for this run.")).toBeInTheDocument();
  });

  it("shows candidate-state provenance and run lineage as independently displayed, distinct fields", () => {
    // parent_run_id and candidate_source_run_id deliberately point at
    // DIFFERENT runs here — this component must never conflate them or
    // derive one from the other.
    const run = baseRun({
      parent_run_id: "run-parent",
      candidate_source_run_id: "run-candidate-source",
      materialization_status: "materialized",
    });
    render(CandidateStateCard, { props: { run } });
    expect(screen.getByText("run-candidate-source")).toBeInTheDocument();
    // This component owns candidate-state only — run lineage is RunPage's
    // own metadata row, never duplicated here.
    expect(screen.queryByText("run-parent")).not.toBeInTheDocument();
  });

  it("points the source-run link at the correct /runs/:id route", () => {
    render(
      CandidateStateCard,
      { props: { run: baseRun({ candidate_source_run_id: "run-abc123" }) } },
    );
    const link = screen.getByRole("link", { name: "run-abc123" });
    expect(link).toHaveAttribute("href", "/runs/run-abc123");
  });

  it("keeps the full tree OID in the DOM text (copyable) even though it is visually truncated", () => {
    const longOid = "a".repeat(64);
    const { container } = render(CandidateStateCard, {
      props: { run: baseRun({ candidate_tree_oid: longOid }) },
    });
    // Real text content is the complete, untruncated value — never a
    // JS-sliced "a1b2c3…" substring — so selecting/copying this element
    // always yields the full OID. The mobile-safety property (this row
    // never forces the page wider than its container) comes from the CSS
    // truncation class below, not from shortening the actual text.
    expect(screen.getByText(longOid)).toBeInTheDocument();
    const oidCell = container.querySelector(".oid");
    expect(oidCell).not.toBeNull();
    expect(oidCell?.textContent).toBe(longOid);
    expect(oidCell).toHaveAttribute("title", longOid);
  });

  it("never renders a mutation control — this is a read-only projection", () => {
    render(CandidateStateCard, {
      props: {
        run: baseRun({
          candidate_source_run_id: "run-x",
          candidate_tree_oid: "deadbeef",
          materialization_status: "materialized",
        }),
      },
    });
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });
});
