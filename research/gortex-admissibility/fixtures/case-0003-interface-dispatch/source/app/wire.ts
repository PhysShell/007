import { AuditSink } from "../adapters/audit-sink";
import { LedgerSink } from "../adapters/ledger-sink";
import type { PaymentSink } from "../ports/payment-sink";

/** Which implementor reaches `settle` is decided by `mode` at runtime. */
export function selectSink(mode: string): PaymentSink {
  return mode === "audit" ? new AuditSink() : new LedgerSink();
}
