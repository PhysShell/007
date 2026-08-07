import { renewToken } from "../index";

export function renew(sessionId: string): string {
  return renewToken(sessionId);
}
