// Shared display formatting — kept out of components so "how do we say how
// old this is" has one answer, not one per page.

export function relativeAge(fromMillis: number, nowMillis: number = Date.now()): string {
  const deltaSeconds = Math.max(0, Math.round((nowMillis - fromMillis) / 1000));
  if (deltaSeconds < 60) return `${deltaSeconds}s`;
  const minutes = Math.round(deltaSeconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.round(minutes / 60);
  if (hours < 24) return `${hours}h`;
  const days = Math.round(hours / 24);
  return `${days}d`;
}

export function duration(startMillis: number, endMillis: number): string {
  const deltaSeconds = Math.max(0, Math.round((endMillis - startMillis) / 1000));
  if (deltaSeconds < 60) return `${deltaSeconds}s`;
  const minutes = Math.floor(deltaSeconds / 60);
  const seconds = deltaSeconds % 60;
  if (minutes < 60) return `${minutes}m ${seconds}s`;
  const hours = Math.floor(minutes / 60);
  const remMinutes = minutes % 60;
  return `${hours}h ${remMinutes}m`;
}

export function absoluteTime(millis: number): string {
  return new Date(millis).toLocaleString();
}
