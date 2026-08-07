import type { PaymentSink } from "../ports/payment-sink";

export class AuditSink implements PaymentSink {
  accept(amountCents: number): string {
    return `audit:${amountCents}`;
  }
}
