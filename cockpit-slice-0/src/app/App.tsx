/* Top-level shell: a conversation list beside a conversation detail pane.
 * Mobile-first — on a phone one pane shows at a time (list, or the selected
 * conversation); at ≥900px both panes show side by side. */
import { useState } from "react";
import type {
  CockpitReadSource,
  CockpitCommandPort,
} from "../data/cockpit-data-source";
import { fixtureDataSource } from "../data/cockpit-data-source";
import { ConversationList } from "../components/ConversationList";
import { ConversationScreen } from "../components/ConversationScreen";
import { useConversationList } from "./useCockpit";

export function App({
  read = fixtureDataSource,
  command = fixtureDataSource,
  initialConversationId = null,
}: {
  read?: CockpitReadSource;
  command?: CockpitCommandPort;
  initialConversationId?: string | null;
}) {
  const conversations = useConversationList(read);
  const [selectedId, setSelectedId] = useState<string | null>(initialConversationId);

  return (
    <div className="app" data-pane={selectedId ? "detail" : "list"}>
      <div className="spike-banner" role="note">
        UI-only spike · fixture-backed · draft / non-mergeable until the canonical
        event protocol is frozen (roadmap PR 4). Not the production Cockpit.
      </div>
      <div className="app__body">
        <aside className="list-pane">
          <ConversationList
            conversations={conversations}
            selectedId={selectedId}
            onSelect={setSelectedId}
          />
        </aside>
        <main className="detail-pane">
          {selectedId ? (
            <ConversationScreen
              read={read}
              command={command}
              conversationId={selectedId}
              onBack={() => setSelectedId(null)}
            />
          ) : (
            <div className="detail-empty">
              <p>Select a conversation.</p>
            </div>
          )}
        </main>
      </div>
    </div>
  );
}
