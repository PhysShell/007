import { describe, it, expect } from "vitest";
import { FixtureCockpitDataSource } from "./cockpit-data-source";
import { foldConversation } from "./fold";

function vmOf(source: FixtureCockpitDataSource, id: string) {
  return foldConversation(id, source.snapshot(id));
}

describe("FixtureCockpitDataSource — read seam", () => {
  it("exposes every scenario as a conversation", () => {
    const s = new FixtureCockpitDataSource();
    expect(s.listConversationIds()).toContain("conv-empty");
    expect(s.listConversationIds()).toContain("conv-replay");
    expect(s.listConversationIds().length).toBeGreaterThanOrEqual(10);
  });

  it("subscribeConversations fires immediately with the current set (dynamic discovery)", () => {
    const s = new FixtureCockpitDataSource();
    let seen: readonly string[] = [];
    const unsub = s.subscribeConversations((ids) => {
      seen = ids;
    });
    expect(seen).toEqual(s.listConversationIds());
    unsub();
  });

  it("subscribers receive delivered batches", () => {
    const s = new FixtureCockpitDataSource();
    const seen: number[] = [];
    const unsub = s.subscribe("conv-empty", (batch) => seen.push(batch.length));
    s.send("conv-empty", "hi");
    expect(seen).toHaveLength(1);
    unsub();
  });
});

describe("FixtureCockpitDataSource — command port (mock, no process)", () => {
  it("send appends a user message", () => {
    const s = new FixtureCockpitDataSource();
    const before = vmOf(s, "conv-empty").timeline.length;
    s.send("conv-empty", "hello");
    const after = vmOf(s, "conv-empty");
    expect(after.timeline.length).toBe(before + 1);
    const last = after.timeline[after.timeline.length - 1];
    expect(last.itemKind).toBe("message");
    if (last.itemKind === "message") {
      expect(last.role).toBe("user");
      expect(last.text).toBe("hello");
    }
  });

  it("setPermission surfaces a requested≠effective gap for bypass", () => {
    const s = new FixtureCockpitDataSource();
    s.setPermission("conv-empty", "bypass");
    const vm = vmOf(s, "conv-empty");
    expect(vm.controls.permission.requested).toBe("bypass");
    expect(vm.controls.permission.effective).toBe("auto");
    expect(vm.controls.permission.mismatch).toBe(true);
  });

  it("reconnect replays the scripted duplicate/out-of-order batch", () => {
    const s = new FixtureCockpitDataSource();
    expect(vmOf(s, "conv-replay").connection.status).toBe("disconnected");
    s.reconnect("conv-replay");
    const vm = vmOf(s, "conv-replay");
    expect(vm.connection.status).toBe("connected");
    expect(vm.runGraph.byId["run-rp-1"].status).toBe("completed");
    // Still no duplicate items after the duplicate-laden replay.
    const keys = vm.timeline.map((i) => i.key);
    expect(new Set(keys).size).toBe(keys.length);
  });
});

describe("architectural invariant — run state is decoupled from clients", () => {
  it("unsubscribing (client unmount) does not change run state", () => {
    const s = new FixtureCockpitDataSource();
    const active = vmOf(s, "conv-claude-active");
    expect(active.activity).toBe("active");
    const runStatusBefore = active.runGraph.byId["run-ca-1"].status;

    const unsub = s.subscribe("conv-claude-active", () => {});
    unsub();

    const afterUnmount = vmOf(s, "conv-claude-active");
    expect(afterUnmount.runGraph.byId["run-ca-1"].status).toBe(runStatusBefore);
    expect(afterUnmount.activity).toBe("active");
  });

  it("state lives in the source, not in any subscriber: a fresh subscriber sees prior mock commands", () => {
    const s = new FixtureCockpitDataSource();
    s.send("conv-empty", "persisted");
    const unsub = s.subscribe("conv-empty", () => {});
    const latestLen = vmOf(s, "conv-empty").timeline.length;
    unsub();
    expect(latestLen).toBe(1);
  });
});
