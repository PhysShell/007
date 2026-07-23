/* The unified timeline. Renders every UI-only item kind: user/agent messages,
 * streaming messages, tool activity, permission requests, artifact/gate/result
 * cards, terminal failures, and recovered historical items. Pure presentational —
 * all state comes from the folded view model. */
import type { UiTimelineItem } from "../types/ui-view-model";
import { AgentBadge } from "./AgentBadge";
import { StatusPill, type UiTone } from "./StatusPill";

function gateTone(status: "running" | "passed" | "failed"): UiTone {
  return status === "passed" ? "good" : status === "failed" ? "bad" : "active";
}
function permTone(status: "pending" | "allowed" | "denied"): UiTone {
  return status === "allowed" ? "good" : status === "denied" ? "bad" : "warn";
}
function toolTone(phase: "requested" | "started" | "completed" | "failed"): UiTone {
  return phase === "completed"
    ? "good"
    : phase === "failed"
      ? "bad"
      : phase === "started"
        ? "active"
        : "waiting";
}

function ItemFrame({
  item,
  hl,
  children,
}: {
  item: UiTimelineItem;
  hl?: "on" | "dim";
  children: React.ReactNode;
}) {
  return (
    <li
      className="tl-item"
      data-kind={item.itemKind}
      data-source={item.source}
      data-run={item.runId ?? ""}
      data-hl={hl ?? ""}
    >
      <div className="tl-item__head">
        <AgentBadge source={item.source} />
        {item.recovered && (
          <span className="chip chip--recovered" title="Recovered from history after a reconnect / restart">
            recovered
          </span>
        )}
      </div>
      <div className="tl-item__body">{children}</div>
    </li>
  );
}

function Item({ item, hl }: { item: UiTimelineItem; hl?: "on" | "dim" }) {
  switch (item.itemKind) {
    case "message":
      return (
        <ItemFrame item={item} hl={hl}>
          <p className="msg" data-role={item.role}>
            {item.text}
            {item.streaming && (
              <span className="stream-caret" aria-label="streaming" role="status">
                ▍
              </span>
            )}
          </p>
        </ItemFrame>
      );
    case "tool":
      return (
        <ItemFrame item={item} hl={hl}>
          <div className="card card--tool">
            <div className="card__row">
              <span className="card__tag">tool · {item.tool}</span>
              <StatusPill tone={toolTone(item.phase)} label={item.phase} />
            </div>
            <div className="card__title">{item.title}</div>
            {item.detail && <div className="card__detail">{item.detail}</div>}
          </div>
        </ItemFrame>
      );
    case "permission":
      return (
        <ItemFrame item={item} hl={hl}>
          <div className="card card--permission" data-status={item.status}>
            <div className="card__row">
              <span className="card__tag">permission</span>
              <StatusPill tone={permTone(item.status)} label={item.status} />
            </div>
            <div className="card__title">{item.tool}</div>
            <div className="card__detail">
              requested mode: <code>{item.requestedMode}</code>
            </div>
            {item.rationale && <div className="card__detail">{item.rationale}</div>}
            {item.status === "pending" && (
              <div className="card__actions" role="group" aria-label="Permission decision (mock)">
                <button type="button" className="btn btn--good" disabled>
                  Allow
                </button>
                <button type="button" className="btn btn--bad" disabled>
                  Deny
                </button>
                <span className="hint">mock — decision is owned by o7d, not the UI</span>
              </div>
            )}
          </div>
        </ItemFrame>
      );
    case "artifact":
      return (
        <ItemFrame item={item} hl={hl}>
          <div className="card card--artifact">
            <div className="card__row">
              <span className="card__tag">artifact · {item.artifactKind}</span>
            </div>
            <div className="card__title">{item.name}</div>
            <div className="card__detail">{item.summary}</div>
          </div>
        </ItemFrame>
      );
    case "gate":
      return (
        <ItemFrame item={item} hl={hl}>
          <div className="card card--gate" data-status={item.status}>
            <div className="card__row">
              <span className="card__tag">gate</span>
              <StatusPill tone={gateTone(item.status)} label={item.status} />
            </div>
            <div className="card__title">{item.name}</div>
            {item.detail && <div className="card__detail">{item.detail}</div>}
          </div>
        </ItemFrame>
      );
    case "result":
      return (
        <ItemFrame item={item} hl={hl}>
          <div className="card card--result" data-status={item.status}>
            <div className="card__row">
              <span className="card__tag">o7d verdict</span>
              <StatusPill
                tone={item.status === "accepted" ? "good" : "bad"}
                label={item.verdict}
              />
            </div>
            <div className="card__detail">{item.summary}</div>
          </div>
        </ItemFrame>
      );
    case "failure":
      return (
        <ItemFrame item={item} hl={hl}>
          <div className="card card--failure">
            <div className="card__row">
              <span className="card__tag">terminal failure</span>
              <StatusPill tone="bad" label={item.reason} />
            </div>
            {item.detail && <div className="card__detail">{item.detail}</div>}
          </div>
        </ItemFrame>
      );
  }
}

export function Timeline({
  items,
  highlightRunId,
}: {
  items: readonly UiTimelineItem[];
  highlightRunId?: string;
}) {
  return (
    <ol
      className="timeline"
      aria-label="Conversation timeline"
      data-highlight={highlightRunId ?? ""}
    >
      {items.map((item) => {
        const hl = highlightRunId
          ? item.runId === highlightRunId
            ? "on"
            : "dim"
          : undefined;
        return <Item key={item.key} item={item} hl={hl} />;
      })}
    </ol>
  );
}
