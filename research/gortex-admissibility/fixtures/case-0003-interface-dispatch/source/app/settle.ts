import type { PaymentSink } from "../ports/payment-sink";

export function settle(sink: PaymentSink, amountCents: number): string {
  return sink.accept(amountCents);
}
