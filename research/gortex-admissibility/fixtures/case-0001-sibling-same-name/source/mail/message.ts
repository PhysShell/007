export class MailMessage {
  constructor(private readonly to: string) {}

  /** Sibling `send` -- unrelated to Invoice.send beyond the name. */
  send(): string {
    return `mail:${this.to}`;
  }
}
