> DOMAIN-OWNED PLACEHOLDER. The OwnAudit agent replaces this with the real
> taxonomy per category (subscription-leak / idisposable / region-escape).
> o7 only carries the slot — the criteria are yours.

## subscription-leak (OWN001, category 2)
A finding means: an event handler is subscribed but no unsubscribe was found.

Classify **fp** when any holds:
- the event source's lifetime is clearly <= the subscriber's (e.g. a local object
  created and owned in the same scope and disposed there);
- an unsubscribe covers this handler (`-=` in `Dispose`/`Unloaded`/`Closed`, or a
  `WeakEventManager` / weak-event pattern);
- the subscriber is itself long-lived/singleton, so retention is intended.

Classify **real** when: the source lifetime is unknown or longer (injected
dependency, static, application-level) AND no unsubscribe path exists AND the
subscriber (View/ViewModel) is the object being retained.

Classify **uncertain** when lifetimes cannot be determined from this file alone.
