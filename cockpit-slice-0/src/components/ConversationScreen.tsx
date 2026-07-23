/* One conversation: header, a tabbed body (Timeline · Runs · Controls), and the
 * composer docked at the bottom. This is a seam consumer — it reads the live view
 * model via useConversation and dispatches mock actions — but owns no process. */
import { useEffect, useState } from "react";
import type { CockpitEventSource } from "../data/cockpit-data-source";
import type { UiPermissionMode } from "../types/ui-fixture-events";
import { useConversation } from "../app/useCockpit";
import { Timeline } from "./Timeline";
import { RunGraph } from "./RunGraph";
import { Composer } from "./Composer";
import { ControlsPanel } from "./ControlsPanel";
import { StatusPill, type UiTone } from "./StatusPill";
import { AgentBadge } from "./AgentBadge";

type Tab = "timeline" | "runs" | "controls";

function connTone(status: string): UiTone {
  if (status === "connected") return "good";
  if (status === "replaying") return "active";
  if (status === "reconnecting") return "warn";
  return "bad";
}

export function ConversationScreen({
  source,
  conversationId,
  onBack,
}: {
  source: CockpitEventSource;
  conversationId: string;
  onBack?: () => void;
}) {
  const vm = useConversation(source, conversationId);
  const [tab, setTab] = useState<Tab>("timeline");
  const [selectedRunId, setSelectedRunId] = useState<string | undefined>();

  // Reset transient view state when switching conversations.
  useEffect(() => {
    setTab("timeline");
    setSelectedRunId(undefined);
  }, [conversationId]);

  const jumpToRun = (runId: string) => {
    setSelectedRunId(runId);
    setTab("timeline");
  };

  return (
    <section className="conv-screen" aria-label={`Conversation: ${vm.title}`}>
      <header className="conv-screen__header">
        {onBack && (
          <button type="button" className="icon-btn back" aria-label="Back to conversations" onClick={onBack}>
            ‹
          </button>
        )}
        <div className="conv-screen__heading">
          <h2 className="conv-screen__title">{vm.title}</h2>
          <div className="conv-screen__meta">
            {vm.agents.map((a) => (
              <AgentBadge key={a} source={a} />
            ))}
            <StatusPill tone={connTone(vm.connection.status)} label={vm.connection.status} />
            {vm.activity === "active" && <StatusPill tone="active" label="run active" />}
          </div>
        </div>
      </header>

      <div className="tabs" role="tablist" aria-label="Conversation views">
        {(["timeline", "runs", "controls"] as const).map((t) => (
          <button
            key={t}
            role="tab"
            type="button"
            aria-selected={tab === t}
            className="tab"
            data-active={tab === t}
            onClick={() => setTab(t)}
          >
            {t === "timeline" ? "Timeline" : t === "runs" ? "Runs" : "Controls"}
          </button>
        ))}
      </div>

      <div className="conv-screen__body" role="tabpanel">
        {tab === "timeline" && <TimelineTab vm={vm} highlightRunId={selectedRunId} />}
        {tab === "runs" && (
          <RunGraph graph={vm.runGraph} onSelectRun={jumpToRun} selectedRunId={selectedRunId} />
        )}
        {tab === "controls" && (
          <ControlsPanel
            controls={vm.controls}
            connection={vm.connection}
            onSetPermission={(mode: UiPermissionMode) =>
              source.dispatch({ type: "controls.setPermission", conversationId, mode })
            }
            onSetModelLock={(locked) =>
              source.dispatch({ type: "controls.setModelLock", conversationId, locked })
            }
            onReconnect={() => source.dispatch({ type: "connection.reconnect", conversationId })}
          />
        )}
      </div>

      <Composer
        state={vm.composer}
        onSend={(text) => source.dispatch({ type: "composer.send", conversationId, text })}
        onStop={() => source.dispatch({ type: "composer.stop", conversationId })}
      />
    </section>
  );
}

function TimelineTab({
  vm,
  highlightRunId,
}: {
  vm: ReturnType<typeof useConversation>;
  highlightRunId?: string;
}) {
  if (vm.loadState === "loading") {
    return (
      <div className="state-note" role="status">
        <span className="spinner" aria-hidden="true" /> Loading conversation…
      </div>
    );
  }
  if (vm.loadState === "error") {
    return (
      <div className="state-note state-note--error" role="alert">
        <strong>Couldn’t load this conversation.</strong>
        <div>{vm.errorMessage ?? "Unknown error."}</div>
      </div>
    );
  }
  if (vm.loadState === "empty") {
    return (
      <div className="state-note" role="status">
        <strong>No messages yet.</strong>
        <div>Send the first message to start a run.</div>
      </div>
    );
  }
  return <Timeline items={vm.timeline} highlightRunId={highlightRunId} />;
}
