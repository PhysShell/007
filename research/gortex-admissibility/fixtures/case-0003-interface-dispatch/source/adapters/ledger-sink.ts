import type { PaymentSink } from "../ports/payment-sink";

export class LedgerSink implements PaymentSink {
  accept(amountCents: number): string {
    return `ledger:${amountCents}`;
  }
}
