import { ApiClient } from "../generated/api-client";

export function loadInvoice(client: ApiClient, id: string): Promise<string> {
  return client.fetchInvoice(id);
}
