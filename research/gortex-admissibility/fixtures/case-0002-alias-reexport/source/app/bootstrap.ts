import { refreshToken } from "../core/token";

export function bootstrap(sessionId: string): string {
  return refreshToken(sessionId);
}
