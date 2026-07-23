import { describe, it, expect } from "vitest";
import { render, screen, within, cleanup } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { axe } from "jest-axe";
import { App } from "../app/App";
import { ConversationScreen } from "./ConversationScreen";
import { FixtureCockpitDataSource } from "../data/cockpit-data-source";
import { foldConversation } from "../data/fold";

const fresh = () => new FixtureCockpitDataSource();

describe("conversation navigation", () => {
  it("lists conversations with unread + activity indicators", () => {
    render(<App source={fresh()} />);
    expect(screen.getByText("Refactor the auth module")).toBeInTheDocument();
    expect(screen.getByText("Ship the retry-policy change")).toBeInTheDocument();
    // Activity dot present for an active conversation.
    expect(screen.getAllByLabelText("active").length).toBeGreaterThan(0);
    // Unread badge.
    expect(screen.getAllByLabelText(/unread/).length).toBeGreaterThan(0);
  });

  it("opens a conversation on selection", async () => {
    const user = userEvent.setup();
    render(<App source={fresh()} />);
    await user.click(screen.getByRole("button", { name: /Refactor the auth module/ }));
    expect(
      screen.getByRole("region", { name: /Conversation: Refactor the auth module/ })
    ).toBeInTheDocument();
  });
});

describe("timeline states", () => {
  it("renders the empty state and enables the composer", () => {
    render(<App source={fresh()} initialConversationId="conv-empty" />);
    expect(screen.getByText("No messages yet.")).toBeInTheDocument();
    expect(screen.getByLabelText("Message the agent")).not.toBeDisabled();
  });

  it("sending a message (mock action) appends it to the timeline", async () => {
    const user = userEvent.setup();
    render(<App source={fresh()} initialConversationId="conv-empty" />);
    await user.type(screen.getByLabelText("Message the agent"), "do the thing");
    await user.click(screen.getByRole("button", { name: /Send message/ }));
    // Scope to the conversation region (the list pane also shows it as a preview).
    const region = screen.getByRole("region", { name: /Conversation:/ });
    expect(within(region).getByText("do the thing")).toBeInTheDocument();
  });

  it("shows a streaming caret for an active streaming message", () => {
    render(<App source={fresh()} initialConversationId="conv-claude-active" />);
    expect(screen.getByRole("status", { name: "streaming" })).toBeInTheDocument();
  });

  it("shows a pending permission request card", () => {
    render(<App source={fresh()} initialConversationId="conv-permission" />);
    expect(
      screen.getByText("bash: rm -rf db/migrations/legacy")
    ).toBeInTheDocument();
    expect(screen.getAllByText("pending").length).toBeGreaterThan(0);
  });

  it("shows a terminal failure + rejected result for a verifier failure", () => {
    render(<App source={fresh()} initialConversationId="conv-verifier-failure" />);
    expect(screen.getByText("Verifier gate failed")).toBeInTheDocument();
    expect(screen.getByText("REJECTED")).toBeInTheDocument();
  });

  it("shows an artifact + passing gate + accepted result", () => {
    render(<App source={fresh()} initialConversationId="conv-artifact-gate" />);
    expect(screen.getByText(/feat\/judge-jobs @ a1b2c3d/)).toBeInTheDocument();
    expect(screen.getByText("ACCEPTED")).toBeInTheDocument();
    expect(screen.getByText("passed")).toBeInTheDocument();
  });

  it("marks recovered historical items", () => {
    render(<App source={fresh()} initialConversationId="conv-interrupted" />);
    expect(screen.getAllByText("recovered").length).toBeGreaterThan(0);
    expect(screen.getByText("INTERRUPTED_BY_HOST_RESTART")).toBeInTheDocument();
  });
});

describe("run graph", () => {
  it("nests a Codex child under its Claude parent and jumps to a run", async () => {
    const user = userEvent.setup();
    render(<App source={fresh()} initialConversationId="conv-delegation" />);
    await user.click(screen.getByRole("tab", { name: "Runs" }));
    const childRun = screen.getByRole("button", {
      name: /Run Codex · write tests/,
    });
    expect(childRun).toBeInTheDocument();
    await user.click(childRun);
    // Jumps back to the timeline tab.
    expect(screen.getByRole("tab", { name: "Timeline" })).toHaveAttribute(
      "aria-selected",
      "true"
    );
  });
});

describe("controls", () => {
  it("shows requested vs effective model drift", async () => {
    const user = userEvent.setup();
    render(<App source={fresh()} initialConversationId="conv-model-mismatch" />);
    await user.click(screen.getByRole("tab", { name: "Controls" }));
    expect(screen.getByText("model drift")).toBeInTheDocument();
    expect(screen.getByText("claude-sonnet-4")).toBeInTheDocument();
  });

  it("permission selector is interactive in the mock state", async () => {
    const user = userEvent.setup();
    const source = fresh();
    render(<App source={source} initialConversationId="conv-empty" />);
    await user.click(screen.getByRole("tab", { name: "Controls" }));
    await user.click(screen.getByRole("radio", { name: "auto" }));
    expect(screen.getByRole("radio", { name: "auto" })).toHaveAttribute(
      "aria-checked",
      "true"
    );
  });

  it("offline conversation disables the composer", () => {
    render(<App source={fresh()} initialConversationId="conv-replay" />);
    expect(screen.getByLabelText("Message the agent")).toBeDisabled();
    expect(screen.getByText(/Offline —/)).toBeInTheDocument();
  });
});

describe("architectural invariant (via React lifecycle)", () => {
  it("unmounting the conversation client does not change the mock run state", () => {
    const source = fresh();
    const before = foldConversation("conv-claude-active", source.snapshot("conv-claude-active"));
    expect(before.runGraph.byId["run-ca-1"].status).toBe("running");

    const view = render(
      <ConversationScreen source={source} conversationId="conv-claude-active" />
    );
    view.unmount(); // client closes / unmounts

    const after = foldConversation("conv-claude-active", source.snapshot("conv-claude-active"));
    expect(after.runGraph.byId["run-ca-1"].status).toBe("running");
    expect(after.activity).toBe("active");
  });
});

describe("accessibility smoke checks", () => {
  it("the conversation list has no axe violations", async () => {
    const { container } = render(<App source={fresh()} />);
    expect(await axe(container)).toHaveNoViolations();
    cleanup();
  });

  it("a conversation timeline has no axe violations", async () => {
    const { container } = render(
      <App source={fresh()} initialConversationId="conv-artifact-gate" />
    );
    expect(await axe(container)).toHaveNoViolations();
    cleanup();
  });

  it("the controls view has no axe violations", async () => {
    const user = userEvent.setup();
    const { container } = render(
      <App source={fresh()} initialConversationId="conv-permission" />
    );
    await user.click(screen.getByRole("tab", { name: "Controls" }));
    expect(await axe(container)).toHaveNoViolations();
    cleanup();
  });
});
