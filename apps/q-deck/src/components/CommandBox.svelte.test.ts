// Q-Deck R1 (docs/q-deck/r1-command.md §9.7): the minimal command UI's own
// contract — no double-submission, an idempotency key reused across a
// retry of the same text, the field cleared only after durable acceptance
// (202), and an error must never erase what the user typed.

import { describe, expect, it, vi, afterEach } from "vitest";
import { render, fireEvent, waitFor } from "@testing-library/svelte";
import CommandBox from "./CommandBox.svelte";
import * as api from "../lib/api";
import { ApiError } from "../lib/api";
import type { CommandAcceptedDto } from "../lib/types";

function accepted(overrides: Partial<CommandAcceptedDto> = {}): CommandAcceptedDto {
  return {
    schema_version: 1,
    command_id: "cmd-1",
    conversation_id: "conv-1",
    parent_run_id: "run-1",
    run_id: "run-2",
    status: "accepted",
    ...overrides,
  };
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("CommandBox", () => {
  it("disables the box entirely when there is no run yet to continue from", () => {
    const { getByRole, getByPlaceholderText } = render(CommandBox, {
      props: { conversationId: "conv-1", parentRunId: null },
    });
    expect(getByRole("button", { name: /send/i })).toBeDisabled();
    expect(getByPlaceholderText(/send a follow-up command/i)).toBeDisabled();
  });

  it("sends the command and clears the field only after durable acceptance", async () => {
    const spy = vi.spyOn(api, "postCommand").mockResolvedValue(accepted());
    const { getByRole, getByPlaceholderText } = render(CommandBox, {
      props: { conversationId: "conv-1", parentRunId: "run-1" },
    });

    const textarea = getByPlaceholderText(/send a follow-up command/i) as HTMLTextAreaElement;
    await fireEvent.input(textarea, { target: { value: "run the tests" } });
    await fireEvent.click(getByRole("button", { name: /send/i }));

    await waitFor(() => expect(textarea.value).toBe(""));
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenCalledWith(
      "conv-1",
      expect.objectContaining({ parentRunId: "run-1", command: "run the tests" }),
    );
  });

  it("shows the server's rejection reason and preserves the typed text", async () => {
    vi.spyOn(api, "postCommand").mockRejectedValue(
      new ApiError(409, {
        schema_version: 1,
        error: "another command is already in flight",
        code: "COMMAND_CONFLICT",
      }),
    );
    const { getByRole, getByPlaceholderText, findByRole } = render(CommandBox, {
      props: { conversationId: "conv-1", parentRunId: "run-1" },
    });

    const textarea = getByPlaceholderText(/send a follow-up command/i) as HTMLTextAreaElement;
    await fireEvent.input(textarea, { target: { value: "run the tests" } });
    await fireEvent.click(getByRole("button", { name: /send/i }));

    const alert = await findByRole("alert");
    expect(alert.textContent).toMatch(/already in flight/i);
    // The whole point: a rejection must never discard what the user typed.
    expect(textarea.value).toBe("run the tests");
  });

  it("cannot be double-submitted while a request is in flight", async () => {
    let resolveCall: (v: CommandAcceptedDto) => void = () => {};
    const spy = vi.spyOn(api, "postCommand").mockImplementation(
      () => new Promise((resolve) => (resolveCall = resolve)),
    );
    const { getByRole, getByPlaceholderText } = render(CommandBox, {
      props: { conversationId: "conv-1", parentRunId: "run-1" },
    });

    const textarea = getByPlaceholderText(/send a follow-up command/i) as HTMLTextAreaElement;
    await fireEvent.input(textarea, { target: { value: "run the tests" } });
    const button = getByRole("button", { name: /send/i });
    await fireEvent.click(button);
    await waitFor(() => expect(button).toBeDisabled());
    // A second click while pending (e.g. an impatient extra tap) must not
    // fire a second request.
    await fireEvent.click(button);
    resolveCall(accepted());
    await waitFor(() => expect(textarea.value).toBe(""));

    expect(spy).toHaveBeenCalledTimes(1);
  });

  it("reuses the same idempotency key across a retry of the identical text", async () => {
    const spy = vi
      .spyOn(api, "postCommand")
      .mockRejectedValueOnce(new Error("network error"))
      .mockResolvedValueOnce(accepted());
    const { getByRole, getByPlaceholderText, findByRole } = render(CommandBox, {
      props: { conversationId: "conv-1", parentRunId: "run-1" },
    });

    const textarea = getByPlaceholderText(/send a follow-up command/i) as HTMLTextAreaElement;
    await fireEvent.input(textarea, { target: { value: "run the tests" } });
    const button = getByRole("button", { name: /send/i });
    await fireEvent.click(button);
    await findByRole("alert");
    await fireEvent.click(button); // retry, text unchanged
    await waitFor(() => expect(textarea.value).toBe(""));

    expect(spy).toHaveBeenCalledTimes(2);
    const firstKey = spy.mock.calls[0][1].idempotencyKey;
    const secondKey = spy.mock.calls[1][1].idempotencyKey;
    expect(secondKey).toBe(firstKey);
  });

  it("mints a fresh idempotency key when the text is edited after a failure", async () => {
    const spy = vi
      .spyOn(api, "postCommand")
      .mockRejectedValueOnce(new Error("network error"))
      .mockResolvedValueOnce(accepted());
    const { getByRole, getByPlaceholderText, findByRole } = render(CommandBox, {
      props: { conversationId: "conv-1", parentRunId: "run-1" },
    });

    const textarea = getByPlaceholderText(/send a follow-up command/i) as HTMLTextAreaElement;
    await fireEvent.input(textarea, { target: { value: "run the tests" } });
    const button = getByRole("button", { name: /send/i });
    await fireEvent.click(button);
    await findByRole("alert");
    await fireEvent.input(textarea, { target: { value: "run the tests, again" } });
    await fireEvent.click(button);
    await waitFor(() => expect(textarea.value).toBe(""));

    const firstKey = spy.mock.calls[0][1].idempotencyKey;
    const secondKey = spy.mock.calls[1][1].idempotencyKey;
    expect(secondKey).not.toBe(firstKey);
  });
});
