/*
 * The ONE impure hook layer. Everything else in the presentation tree is a pure
 * function of props. These hooks read from the CockpitReadSource seam and run the
 * fold; they never touch a transport, a worker, or a process. Unmounting a hook
 * only unsubscribes — it cannot change the mock run state (that lives in the data
 * source, decoupled from React lifetime). Commands go through CockpitCommandPort,
 * never through these hooks.
 */
import { useEffect, useRef, useState } from "react";
import type { CockpitReadSource } from "../data/cockpit-data-source";
import {
  applyEvents,
  emptyFoldState,
  projectConversation,
  summarize,
  type UiFoldState,
} from "../data/fold";
import type {
  UiConversationSummary,
  UiConversationViewModel,
} from "../types/ui-view-model";

export function useConversationList(
  read: CockpitReadSource
): UiConversationSummary[] {
  const [summaries, setSummaries] = useState<UiConversationSummary[]>([]);

  useEffect(() => {
    const folds = new Map<string, UiFoldState>();
    const eventUnsubs = new Map<string, () => void>();
    let ids: readonly string[] = [];

    const recompute = () =>
      setSummaries(
        ids.map((cid) => summarize(projectConversation(cid, folds.get(cid)!)))
      );

    // Dynamic conversation discovery: react to the SET of conversations changing.
    const unsubConversations = read.subscribeConversations((nextIds) => {
      ids = nextIds;
      for (const cid of nextIds) {
        if (folds.has(cid)) continue;
        folds.set(cid, applyEvents(emptyFoldState(), read.snapshot(cid)));
        eventUnsubs.set(
          cid,
          read.subscribe(cid, (batch) => {
            folds.set(cid, applyEvents(folds.get(cid)!, batch));
            recompute();
          })
        );
      }
      recompute();
    });

    return () => {
      unsubConversations();
      for (const unsub of eventUnsubs.values()) unsub();
    };
  }, [read]);

  return summaries;
}

export function useConversation(
  read: CockpitReadSource,
  conversationId: string
): UiConversationViewModel {
  const foldRef = useRef<UiFoldState>(emptyFoldState());
  const [vm, setVm] = useState<UiConversationViewModel>(() =>
    projectConversation(
      conversationId,
      applyEvents(emptyFoldState(), read.snapshot(conversationId))
    )
  );

  useEffect(() => {
    foldRef.current = applyEvents(
      emptyFoldState(),
      read.snapshot(conversationId)
    );
    setVm(projectConversation(conversationId, foldRef.current));

    const unsub = read.subscribe(conversationId, (batch) => {
      foldRef.current = applyEvents(foldRef.current, batch);
      setVm(projectConversation(conversationId, foldRef.current));
    });
    return unsub;
  }, [read, conversationId]);

  return vm;
}
