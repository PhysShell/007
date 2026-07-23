/* The run graph: parent/child runs and delegation branches, each with its
 * lifecycle status. Clicking a run jumps to it in the timeline. Presentational. */
import type { UiRunGraph, UiRunNode } from "../types/ui-view-model";
import { AgentBadge } from "./AgentBadge";
import { StatusPill, runTone } from "./StatusPill";

function RunNodeView({
  node,
  depth,
  onSelectRun,
  selectedRunId,
}: {
  node: UiRunNode;
  depth: number;
  onSelectRun?: (runId: string) => void;
  selectedRunId?: string;
}) {
  return (
    <li className="run-node" data-depth={depth}>
      <button
        type="button"
        className="run-node__row"
        aria-current={selectedRunId === node.runId ? "true" : undefined}
        aria-label={`Run ${node.label}, status ${node.status}. Jump to it in the timeline.`}
        onClick={() => onSelectRun?.(node.runId)}
      >
        <AgentBadge source={node.agent} />
        <span className="run-node__label">{node.label}</span>
        {node.role && <span className="chip">{node.role}</span>}
        <StatusPill tone={runTone(node.status)} label={node.status} />
      </button>
      {node.children.length > 0 && (
        <ul className="run-node__children">
          {node.children.map((child) => (
            <RunNodeView
              key={child.runId}
              node={child}
              depth={depth + 1}
              onSelectRun={onSelectRun}
              selectedRunId={selectedRunId}
            />
          ))}
        </ul>
      )}
    </li>
  );
}

export function RunGraph({
  graph,
  onSelectRun,
  selectedRunId,
}: {
  graph: UiRunGraph;
  onSelectRun?: (runId: string) => void;
  selectedRunId?: string;
}) {
  if (graph.roots.length === 0) {
    return <p className="empty-note">No runs yet.</p>;
  }
  return (
    <ul className="run-graph" aria-label="Run graph">
      {graph.roots.map((root) => (
        <RunNodeView
          key={root.runId}
          node={root}
          depth={0}
          onSelectRun={onSelectRun}
          selectedRunId={selectedRunId}
        />
      ))}
    </ul>
  );
}
