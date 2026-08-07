export interface PaymentSink {
  accept(amountCents: number): string;
}
