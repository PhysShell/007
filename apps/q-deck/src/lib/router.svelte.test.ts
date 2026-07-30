import { describe, expect, it } from "vitest";
import { resolve } from "./router.svelte";

describe("resolve", () => {
  it("matches the dashboard root", () => {
    expect(resolve("/")).toEqual({ name: "dashboard" });
  });

  it("matches a run route and decodes the id", () => {
    expect(resolve("/runs/abc-123")).toEqual({ name: "run", runId: "abc-123" });
    expect(resolve("/runs/has%20space")).toEqual({ name: "run", runId: "has space" });
  });

  it("matches a conversation route", () => {
    expect(resolve("/conversations/xyz")).toEqual({ name: "conversation", conversationId: "xyz" });
  });

  it("falls back to not-found for anything else", () => {
    expect(resolve("/nonsense")).toEqual({ name: "not-found" });
    expect(resolve("/runs/")).toEqual({ name: "not-found" });
    expect(resolve("/runs/one/two")).toEqual({ name: "not-found" });
  });
});
