import { describe, it, expect } from "vitest";
import {
  applyEvents,
  emptyFoldState,
  foldConversation,
  projectConversation,
  summarize,
} from "./fold";
import type { UiFixtureEvent } from "../types/ui-fixture-events";
import { SCENARIOS_BY_ID } from "../fixtures/catalog";

// A fixed, deterministic reordering (no Math.random) that still scrambles order.
function scramble<T>(items: readonly T[]): T[] {
  const out = items.slice();
  for (let i = 0; i < out.length - 1; i += 2) {
    const tmp = out[i];
    out[i] = out[i + 1];
    out[i + 1] = tmp;
  }
  return out.reverse();
}

const seqs = (events: readonly UiFixtureEvent[]) => events.map((e) => e.uiSeq);

describe("fold — dedup + ordering (the core guarantees)", () => {
  it("orders strictly by uiSeq regardless of delivery order", () => {
    const c = "c1";
    const events: UiFixtureEvent[] = [
      { kind: "conversationMeta", id: "m", conversationId: c, uiSeq: 1, title: "T" },
      { kind: "userMessage", id: "u1", conversationId: c, uiSeq: 4, text: "fourth" },
      { kind: "userMessage", id: "u2", conversationId: c, uiSeq: 2, text: "second" },
      { kind: "userMessage", id: "u3", conversationId: c, uiSeq: 3, text: "third" },
    ];
    const state = applyEvents(emptyFoldState(), scramble(events));
    expect(seqs(state.orderedEvents)).toEqual([1, 2, 3, 4]);
    const vm = projectConversation(c, state);
    expect(vm.timeline.map((i) => (i.itemKind === "message" ? i.text : ""))).toEqual([
      "second",
      "third",
      "fourth",
    ]);
  });

  it("drops duplicate ids (idempotent replay)", () => {
    const c = "c2";
    const ev: UiFixtureEvent = {
      kind: "userMessage",
      id: "dup",
      conversationId: c,
      uiSeq: 5,
      text: "once",
    };
    const state = applyEvents(emptyFoldState(), [ev, ev, ev]);
    expect(state.orderedEvents).toHaveLength(1);
  });

  it("is delivery-order-independent: scrambled == in-order for every scenario", () => {
    for (const scenario of Object.values(SCENARIOS_BY_ID)) {
      const all = [
        ...scenario.initialDelivery,
        ...(scenario.replay?.batch ?? []),
      ];
      const inOrder = foldConversation(scenario.id, all);
      const scrambled = projectConversation(
        scenario.id,
        applyEvents(emptyFoldState(), scramble(all))
      );
      expect(scrambled.timeline).toEqual(inOrder.timeline);
      expect(scrambled.runGraph).toEqual(inOrder.runGraph);
    }
  });
});

describe("fold — replay scenario reconciles duplicates + out-of-order tail", () => {
  const scenario = SCENARIOS_BY_ID["conv-replay"];

  it("pre-disconnect shows a streaming message and a disconnected transport", () => {
    const vm = foldConversation(scenario.id, scenario.initialDelivery);
    expect(vm.connection.status).toBe("disconnected");
    expect(vm.connection.offline).toBe(true);
    expect(vm.composer.offline).toBe(true);
    const streaming = vm.timeline.find(
      (i) => i.itemKind === "message" && i.role === "agent"
    );
    expect(streaming?.itemKind).toBe("message");
    if (streaming?.itemKind === "message") expect(streaming.streaming).toBe(true);
  });

  it("after replay: deduped, ordered, stream completed, run completed, accepted", () => {
    const vm = foldConversation(scenario.id, [
      ...scenario.initialDelivery,
      ...scenario.replay!.batch,
    ]);
    // No duplicate timeline items despite duplicate deliveries.
    const keys = vm.timeline.map((i) => i.key);
    expect(new Set(keys).size).toBe(keys.length);
    // Streaming message concatenated in uiSeq order and finalized.
    const agentMsg = vm.timeline.find(
      (i) => i.itemKind === "message" && i.role === "agent"
    );
    if (agentMsg?.itemKind === "message") {
      expect(agentMsg.streaming).toBe(false);
      expect(agentMsg.text).toBe(
        "Found the sequential loop; wiring a bounded semaphore around the per-file calls."
      );
    }
    expect(vm.connection.status).toBe("connected");
    expect(vm.runGraph.byId["run-rp-1"].status).toBe("completed");
    expect(
      vm.timeline.some((i) => i.itemKind === "result" && i.status === "accepted")
    ).toBe(true);
  });
});

describe("fold — grouped items", () => {
  it("collapses tool phases by toolCallId to the latest phase", () => {
    const c = "ct";
    const vm = foldConversation(c, [
      { kind: "toolActivity", id: "t1", conversationId: c, uiSeq: 1, source: "claude", toolCallId: "x", tool: "edit", phase: "started", title: "Editing" },
      { kind: "toolActivity", id: "t2", conversationId: c, uiSeq: 2, source: "claude", toolCallId: "x", tool: "edit", phase: "completed", title: "Edited" },
    ]);
    const tools = vm.timeline.filter((i) => i.itemKind === "tool");
    expect(tools).toHaveLength(1);
    if (tools[0].itemKind === "tool") expect(tools[0].phase).toBe("completed");
  });
});

describe("fold — run graph parent/child", () => {
  it("nests a Codex child under its Claude parent", () => {
    const vm = foldConversation(
      "conv-delegation",
      SCENARIOS_BY_ID["conv-delegation"].initialDelivery
    );
    expect(vm.runGraph.roots).toHaveLength(1);
    const parent = vm.runGraph.roots[0];
    expect(parent.runId).toBe("run-parent");
    expect(parent.agent).toBe("claude");
    expect(parent.children).toHaveLength(1);
    expect(parent.children[0].runId).toBe("run-child");
    expect(parent.children[0].agent).toBe("codex");
  });
});

describe("fold — controls, model drift, load states", () => {
  it("flags model mismatch (drift) when requested ≠ effective", () => {
    const vm = foldConversation(
      "conv-model-mismatch",
      SCENARIOS_BY_ID["conv-model-mismatch"].initialDelivery
    );
    expect(vm.controls.model.mismatch).toBe(true);
    expect(vm.controls.model.locked).toBe(true);
  });

  it("derives empty load state for a conversation with no timeline items", () => {
    const vm = foldConversation(
      "conv-empty",
      SCENARIOS_BY_ID["conv-empty"].initialDelivery
    );
    expect(vm.loadState).toBe("empty");
    expect(vm.composer.canSend).toBe(true);
    expect(summarize(vm).lastPreview).toBe("No messages yet");
  });

  it("honors explicit loading / error overrides", () => {
    const c = "cx";
    const loading = foldConversation(c, [
      { kind: "conversationMeta", id: "m", conversationId: c, uiSeq: 1, title: "L", loadState: "loading" },
    ]);
    expect(loading.loadState).toBe("loading");
    const errored = foldConversation(c, [
      { kind: "conversationMeta", id: "m2", conversationId: c, uiSeq: 1, title: "E", loadState: "error", errorMessage: "boom" },
    ]);
    expect(errored.loadState).toBe("error");
    expect(errored.errorMessage).toBe("boom");
  });

  it("marks activity active while a run is running/queued/waiting", () => {
    const vm = foldConversation(
      "conv-claude-active",
      SCENARIOS_BY_ID["conv-claude-active"].initialDelivery
    );
    expect(vm.activity).toBe("active");
    expect(vm.composer.canStop).toBe(true);
  });

  it("tags recovered historical items", () => {
    const vm = foldConversation(
      "conv-interrupted",
      SCENARIOS_BY_ID["conv-interrupted"].initialDelivery
    );
    expect(vm.timeline.some((i) => i.recovered)).toBe(true);
    expect(
      vm.timeline.some(
        (i) => i.itemKind === "failure" && i.reason === "INTERRUPTED_BY_HOST_RESTART"
      )
    ).toBe(true);
    expect(vm.runGraph.byId["run-in-1"].status).toBe("interrupted");
  });
});
