/** The real definition. Every rename must reach every call site below. */
export function refreshToken(sessionId: string): string {
  return `refreshed:${sessionId}`;
}
