import { Invoice } from "../billing/invoice";
import { MailMessage } from "../mail/message";

export function dispatchInvoice(invoice: Invoice): string {
  return invoice.send();
}

export function dispatchMail(message: MailMessage): string {
  return message.send();
}
