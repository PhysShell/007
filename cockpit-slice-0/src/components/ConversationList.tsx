/* Conversation navigation: the list of conversations with unread + activity
 * indicators and per-conversation load state. Presentational — summaries come
 * from the folded view models. */
import type { UiConversationSummary } from "../types/ui-view-model";
import { AgentBadge } from "./AgentBadge";

export function ConversationList({
  conversations,
  selectedId,
  onSelect,
}: {
  conversations: readonly UiConversationSummary[];
  selectedId?: string | null;
  onSelect: (id: string) => void;
}) {
  return (
    <nav className="conv-list" aria-label="Conversations">
      <header className="conv-list__header">
        <h1 className="conv-list__title">Cockpit</h1>
        <span className="conv-list__spike" title="Draft / non-mergeable spike">
          slice 0 · spike
        </span>
      </header>
      {conversations.length === 0 ? (
        <p className="empty-note">No conversations.</p>
      ) : (
        <ul className="conv-list__items">
          {conversations.map((c) => (
            <li key={c.id}>
              <button
                type="button"
                className="conv-item"
                data-selected={selectedId === c.id}
                aria-current={selectedId === c.id ? "true" : undefined}
                onClick={() => onSelect(c.id)}
              >
                <div className="conv-item__top">
                  <span className="conv-item__title">{c.title}</span>
                  <span className="conv-item__badges">
                    {c.activity === "active" && (
                      <span
                        className="activity-dot"
                        role="img"
                        title="A run is active"
                        aria-label="active"
                      />
                    )}
                    {c.unread > 0 && (
                      <span className="unread" role="img" aria-label={`${c.unread} unread`}>
                        {c.unread}
                      </span>
                    )}
                  </span>
                </div>
                <div className="conv-item__agents">
                  {c.agents.map((a) => (
                    <AgentBadge key={a} source={a} />
                  ))}
                  {c.loadState !== "ready" && c.loadState !== "empty" && (
                    <span className="chip" data-state={c.loadState}>
                      {c.loadState}
                    </span>
                  )}
                </div>
                <div className="conv-item__preview" data-empty={c.loadState === "empty"}>
                  {c.lastPreview}
                </div>
              </button>
            </li>
          ))}
        </ul>
      )}
    </nav>
  );
}
