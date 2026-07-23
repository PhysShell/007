import { describe, it, expect } from "vitest";
import { FixtureCockpitDataSource } from "./cockpit-data-source";
import { foldConversation } from "./fold";

function vmOf(source: FixtureCockpitDataSource, id: string) {
  return foldConversation(id, source.snapshot(id));
}

describe("FixtureCockpitDataSource — seam behavior", () => {
  it("exposes every scenario as a conversation", () => {
    const s = new FixtureCockpitDataSource();
    expect(s.listConversationIds()).toContain("conv-empty");
    expect(s.listConversationIds()).toContain("conv-replay");
    expect(s.listConversationIds().length).toBeGreaterThanOrEqual(10);
  });

  it("composer.send appends a user message (mock action, no process)", () => {
    const s = new FixtureCockpitDataSource();
    const before = vmOf(s, "conv-empty").timeline.length;
    s.dispatch({ type: "composer.send", conversationId: "conv-empty", text: "hello" });
    const after = vmOf(s, "conv-empty");
    expect(after.timeline.length).toBe(before + 1);
    const last = after.timeline[after.timeline.length - 1];
    expect(last.itemKind).toBe("message");
    if (last.itemKind === "message") {
      expect(last.role).toBe("user");
      expect(last.text).toBe("hello");
    }
  });

  it("controls.setPermission surfaces a requested≠effective gap for bypass", () => {
    const s = new FixtureCockpitDataSource();
    s.dispatch({ type: "controls.setPermission", conversationId: "conv-empty", mode: "bypass" });
    const vm = vmOf(s, "conv-empty");
    expect(vm.controls.permission.requested).toBe("bypass");
    expect(vm.controls.permission.effective).toBe("auto");
    expect(vm.controls.permission.mismatch).toBe(true);
  });

  it("connection.reconnect replays the scripted duplicate/out-of-order batch", () => {
    const s = new FixtureCockpitDataSource();
    expect(vmOf(s, "conv-replay").connection.status).toBe("disconnected");
    s.dispatch({ type: "connection.reconnect", conversationId: "conv-replay" });
    const vm = vmOf(s, "conv-replay");
    expect(vm.connection.status).toBe("connected");
    expect(vm.runGraph.byId["run-rp-1"].status).toBe("completed");
    // Still no duplicate items after the duplicate-laden replay.
    const keys = vm.timeline.map((i) => i.key);
    expect(new Set(keys).size).toBe(keys.length);
  });

  it("subscribers receive delivered batches", () => {
    const s = new FixtureCockpitDataSource();
    const seen: number[] = [];
    const unsub = s.subscribe("conv-empty", (batch) => seen.push(batch.length));
    s.dispatch({ type: "composer.send", conversationId: "conv-empty", text: "hi" });
    expect(seen).toHaveLength(1);
    unsub();
  });

  it("ARCHITECTURAL INVARIANT: unsubscribing (client unmount) does not change run state", () => {
    const s = new FixtureCockpitDataSource();
    // Drive conv-claude-active into an active run and observe it.
    const active = vmOf(s, "conv-claude-active");
    expect(active.activity).toBe("active");
    const runStatusBefore = active.runGraph.byId["run-ca-1"].status;

    // A client subscribes then unmounts (unsubscribes).
    const unsub = s.subscribe("conv-claude-active", () => {});
    unsub();

    // The mock run is UNCHANGED — closing the client did not stop it.
    const afterUnmount = vmOf(s, "conv-claude-active");
    expect(afterUnmount.runGraph.byId["run-ca-1"].status).toBe(runStatusBefore);
    expect(afterUnmount.activity).toBe("active");
  });

  it("state lives in the source, not in any subscriber: a fresh subscriber sees prior mock actions", () => {
    const s = new FixtureCockpitDataSource();
    s.dispatch({ type: "composer.send", conversationId: "conv-empty", text: "persisted" });
    // Brand new subscriber, no shared React state.
    let latestLen = 0;
    const unsub = s.subscribe("conv-empty", () => {});
    latestLen = vmOf(s, "conv-empty").timeline.length;
    unsub();
    expect(latestLen).toBe(1);
  });
});
