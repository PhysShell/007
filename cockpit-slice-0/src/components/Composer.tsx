/* The composer: multiline input, send/stop as fixture-driven mock actions,
 * attachment placeholders, and disabled/offline states. It NEVER starts or stops
 * a real process — send/stop only dispatch mock events through the data source. */
import { useState } from "react";
import type { UiComposerState } from "../types/ui-view-model";

export function Composer({
  state,
  onSend,
  onStop,
}: {
  state: UiComposerState;
  onSend: (text: string) => void;
  onStop: () => void;
}) {
  const [text, setText] = useState("");

  const submit = () => {
    if (!state.canSend) return;
    const trimmed = text.trim();
    if (!trimmed) return;
    onSend(trimmed);
    setText("");
  };

  return (
    <form
      className="composer"
      data-offline={state.offline ? "true" : "false"}
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      {state.offline && (
        <div className="composer__offline" role="status">
          Offline — the run continues on the daemon; you just can’t steer it right now.
        </div>
      )}
      <div className="composer__row">
        <div className="composer__attachments" role="group" aria-label="Attachments (placeholder)">
          <button
            type="button"
            className="icon-btn"
            aria-label="Attach file (placeholder — not wired)"
            title="Attach file (placeholder)"
            disabled
          >
            ＋
          </button>
        </div>
        <label className="sr-only" htmlFor="composer-input">
          Message the agent
        </label>
        <textarea
          id="composer-input"
          className="composer__input"
          rows={2}
          placeholder={state.placeholder}
          value={text}
          disabled={!state.enabled}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
              e.preventDefault();
              submit();
            }
          }}
        />
        <div className="composer__controls">
          {state.canStop ? (
            <button
              type="button"
              className="btn btn--stop"
              onClick={onStop}
              aria-label="Stop the active run (mock action)"
            >
              Stop
            </button>
          ) : (
            <button
              type="submit"
              className="btn btn--send"
              disabled={!state.canSend || text.trim().length === 0}
              aria-label="Send message (mock action)"
            >
              Send
            </button>
          )}
        </div>
      </div>
    </form>
  );
}
