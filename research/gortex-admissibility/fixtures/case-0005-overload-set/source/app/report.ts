import { format } from "../format";

export function formatAmount(cents: number): string {
  return format(cents / 100);
}

export function formatWhen(at: Date): string {
  return format(at);
}
