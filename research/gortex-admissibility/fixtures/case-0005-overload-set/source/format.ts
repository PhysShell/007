export function format(value: number): string;
export function format(value: Date): string;
export function format(value: number | Date): string {
  return typeof value === "number" ? value.toFixed(2) : value.toISOString();
}
