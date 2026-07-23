/* Visual distinction between Claude, Codex, and system/o7d — WITHOUT assuming any
 * wire event shape. The `source` is a UI-only label the fold attaches; the real
 * agent identity/attribution is a PR-4 concern. Kept purely presentational. */
import type { UiAgentFamily } from "../types/ui-fixture-events";

export type UiSource = UiAgentFamily | "user";

const LABEL: Record<UiSource, string> = {
  claude: "Claude",
  codex: "Codex",
  system: "system · o7d",
  user: "You",
};

export function AgentBadge({ source }: { source: UiSource }) {
  return (
    <span className="agent-badge" data-source={source} title={LABEL[source]}>
      <span className="agent-badge__dot" aria-hidden="true" />
      {LABEL[source]}
    </span>
  );
}
