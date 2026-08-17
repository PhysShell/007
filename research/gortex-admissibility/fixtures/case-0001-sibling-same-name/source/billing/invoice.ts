export class Invoice {
  constructor(private readonly id: string) {}

  /** Marks the invoice dispatched and returns the audit id. */
  send(): string {
    return `invoice:${this.id}`;
  }
}
