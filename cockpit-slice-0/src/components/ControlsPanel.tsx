/* Controls: permission selector, requested vs effective model, model-lock
 * indicator, and reconnect/replay status. Interactive in the mock state — changing
 * a control dispatches a fixture event through the data source — but nothing here
 * launches, steers, or kills a real process. */
import type { UiPermissionMode } from "../types/ui-fixture-events";
import type {
  UiControlsState,
  UiConnectionState,
} from "../types/ui-view-model";
import { StatusPill, type UiTone } from "./StatusPill";

function connTone(status: UiConnectionState["status"]): UiTone {
  switch (status) {
    case "connected":
      return "good";
    case "replaying":
      return "active";
    case "reconnecting":
      return "warn";
    case "disconnected":
      return "bad";
  }
}

export function ControlsPanel({
  controls,
  connection,
  onSetPermission,
  onSetModelLock,
  onReconnect,
}: {
  controls: UiControlsState;
  connection: UiConnectionState;
  onSetPermission: (mode: UiPermissionMode) => void;
  onSetModelLock: (locked: boolean) => void;
  onReconnect: () => void;
}) {
  return (
    <div className="controls">
      <section className="controls__section" aria-labelledby="ctl-perm">
        <h3 id="ctl-perm" className="controls__h">
          Permission mode
        </h3>
        <div className="seg" role="radiogroup" aria-label="Permission mode">
          {controls.permission.options.map((mode) => {
            const active = controls.permission.requested === mode;
            return (
              <button
                key={mode}
                type="button"
                role="radio"
                aria-checked={active}
                className="seg__btn"
                data-active={active}
                onClick={() => onSetPermission(mode)}
              >
                {mode}
              </button>
            );
          })}
        </div>
        <div className="controls__reqeff">
          <span>
            requested: <code>{controls.permission.requested}</code>
          </span>
          <span>
            effective: <code>{controls.permission.effective}</code>
          </span>
          {controls.permission.mismatch && (
            <StatusPill
              tone="warn"
              label="requested ≠ effective"
              title="The UI must never show a lit button that lies about the underlying process."
            />
          )}
        </div>
      </section>

      <section className="controls__section" aria-labelledby="ctl-model">
        <h3 id="ctl-model" className="controls__h">
          Model
        </h3>
        <div className="controls__reqeff">
          <span>
            requested: <code>{controls.model.requested || "—"}</code>
          </span>
          <span>
            effective: <code>{controls.model.effective || "—"}</code>
          </span>
          {controls.model.locked && (
            <StatusPill tone="neutral" label="🔒 model-lock" title="Exact-model policy: no silent fallback." />
          )}
          {controls.model.mismatch && (
            <StatusPill tone="bad" label="model drift" title="Requested ≠ effective — the exact-model kill switch would trip." />
          )}
        </div>
        <label className="controls__toggle">
          <input
            type="checkbox"
            checked={controls.model.locked}
            onChange={(e) => onSetModelLock(e.target.checked)}
          />
          Lock to the requested model (no fallback)
        </label>
      </section>

      <section className="controls__section" aria-labelledby="ctl-conn">
        <h3 id="ctl-conn" className="controls__h">
          Connection
        </h3>
        <div className="controls__reqeff">
          <StatusPill tone={connTone(connection.status)} label={connection.status} />
          {typeof connection.replayCursor === "number" && (
            <span>
              replay cursor: <code>{connection.replayCursor}</code>
            </span>
          )}
        </div>
        {connection.detail && <p className="controls__detail">{connection.detail}</p>}
        <button type="button" className="btn" onClick={onReconnect}>
          Reconnect / replay
        </button>
      </section>
    </div>
  );
}
