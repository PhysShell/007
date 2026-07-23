/*
 * The ONE impure hook layer. Everything else in the presentation tree is a pure
 * function of props. These hooks subscribe to the CockpitEventSource seam and run
 * the fold; they never touch a transport, a worker, or a process. Unmounting a
 * hook only unsubscribes — it cannot change the mock run state (that lives in the
 * data source, decoupled from React lifetime).
 */
import { useEffect, useRef, useState } from "react";
import type { CockpitEventSource } from "../data/cockpit-data-source";
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
  source: CockpitEventSource
): UiConversationSummary[] {
  const [summaries, setSummaries] = useState<UiConversationSummary[]>([]);

  useEffect(() => {
    const ids = source.listConversationIds();
    const folds = new Map<string, UiFoldState>();
    for (const cid of ids) {
      folds.set(cid, applyEvents(emptyFoldState(), source.snapshot(cid)));
    }
    const recompute = () =>
      setSummaries(
        ids.map((cid) => summarize(projectConversation(cid, folds.get(cid)!)))
      );
    recompute();

    const unsubs = ids.map((cid) =>
      source.subscribe(cid, (batch) => {
        folds.set(cid, applyEvents(folds.get(cid)!, batch));
        recompute();
      })
    );
    return () => unsubs.forEach((u) => u());
  }, [source]);

  return summaries;
}

export function useConversation(
  source: CockpitEventSource,
  conversationId: string
): UiConversationViewModel {
  const foldRef = useRef<UiFoldState>(emptyFoldState());
  const [vm, setVm] = useState<UiConversationViewModel>(() =>
    projectConversation(
      conversationId,
      applyEvents(emptyFoldState(), source.snapshot(conversationId))
    )
  );

  useEffect(() => {
    foldRef.current = applyEvents(
      emptyFoldState(),
      source.snapshot(conversationId)
    );
    setVm(projectConversation(conversationId, foldRef.current));

    const unsub = source.subscribe(conversationId, (batch) => {
      foldRef.current = applyEvents(foldRef.current, batch);
      setVm(projectConversation(conversationId, foldRef.current));
    });
    return unsub;
  }, [source, conversationId]);

  return vm;
}
