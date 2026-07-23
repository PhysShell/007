/* A small status pill used for run states, gate/result outcomes, and permission
 * status. `tone` drives the color; `label` is the visible text. Presentational. */
export type UiTone = "neutral" | "active" | "waiting" | "good" | "bad" | "warn";

export function StatusPill({
  tone,
  label,
  title,
}: {
  tone: UiTone;
  label: string;
  title?: string;
}) {
  return (
    <span className="status-pill" data-tone={tone} title={title ?? label}>
      {label}
    </span>
  );
}

import type { UiRunStatus } from "../types/ui-fixture-events";

export function runTone(status: UiRunStatus): UiTone {
  switch (status) {
    case "running":
      return "active";
    case "queued":
    case "waiting":
      return "waiting";
    case "cancelling":
      return "warn";
    case "completed":
      return "good";
    case "failed":
      return "bad";
    case "interrupted":
      return "warn";
  }
}
