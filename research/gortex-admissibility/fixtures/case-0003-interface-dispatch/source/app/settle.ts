import type { PaymentSink } from "../ports/payment-sink";
import { selectSink } from "./wire";

/**
 * Not exported. The only way into this call is `run`, so the receiver always
 * arrives from `selectSink` and is never chosen by a caller outside this file.
 */
function settle(sink: PaymentSink, amountCents: number): string {
  return sink.accept(amountCents);
}

export function run(mode: string, amountCents: number): string {
  return settle(selectSink(mode), amountCents);
}
