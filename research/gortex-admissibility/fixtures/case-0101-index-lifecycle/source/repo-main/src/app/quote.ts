import { RateTable } from "../core/rates";

export function quote(table: RateTable, code: string): number {
  return table.lookup(code) * 100;
}
