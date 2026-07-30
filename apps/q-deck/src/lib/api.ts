// One typed client for every o7d call — no endpoint fetch scattered through
// components, so a wire-shape change or a new schema_version has exactly one
// place to update.

import type {
  ConversationDto,
  ErrorDto,
  EventPageDto,
  HealthDto,
  PageDto,
  RunDto,
} from "./types";

export class ApiError extends Error {
  readonly code: string;
  readonly status: number;

  constructor(status: number, body: ErrorDto) {
    super(body.error);
    this.name = "ApiError";
    this.code = body.code;
    this.status = status;
  }
}

/** Thrown when a response's shape isn't what this client understands — e.g. a
 * schema_version this build has never seen. Surfaced explicitly rather than
 * rendered as if the data were merely empty. */
export class UnsupportedSchemaError extends Error {
  constructor(seen: number, expected: number) {
    super(`server schema_version ${seen}, this client understands ${expected}`);
    this.name = "UnsupportedSchemaError";
  }
}

const EXPECTED_SCHEMA_VERSION = 1;

function checkSchema(body: { schema_version: number }): void {
  if (body.schema_version !== EXPECTED_SCHEMA_VERSION) {
    throw new UnsupportedSchemaError(body.schema_version, EXPECTED_SCHEMA_VERSION);
  }
}

async function getJson<T extends { schema_version: number }>(path: string): Promise<T> {
  const resp = await fetch(path);
  if (!resp.ok) {
    const body = (await resp.json()) as ErrorDto;
    throw new ApiError(resp.status, body);
  }
  const body = (await resp.json()) as T;
  checkSchema(body);
  return body;
}

export function health(): Promise<HealthDto> {
  return getJson<HealthDto>("/api/v1/health");
}

export interface ListParams {
  limit?: number;
  before?: string;
}

export function listConversations(params: ListParams = {}): Promise<PageDto<ConversationDto>> {
  const qs = new URLSearchParams();
  if (params.limit !== undefined) qs.set("limit", String(params.limit));
  if (params.before !== undefined) qs.set("before", params.before);
  const suffix = qs.toString() ? `?${qs}` : "";
  return getJson<PageDto<ConversationDto>>(`/api/v1/conversations${suffix}`);
}

export function getConversation(id: string): Promise<ConversationDto> {
  return getJson<ConversationDto>(`/api/v1/conversations/${encodeURIComponent(id)}`);
}

export function getConversationEvents(
  id: string,
  after?: number,
  limit?: number,
): Promise<EventPageDto> {
  const qs = new URLSearchParams();
  if (after !== undefined) qs.set("after", String(after));
  if (limit !== undefined) qs.set("limit", String(limit));
  const suffix = qs.toString() ? `?${qs}` : "";
  return getJson<EventPageDto>(`/api/v1/conversations/${encodeURIComponent(id)}/events${suffix}`);
}

export interface RunsListParams extends ListParams {
  conversationId?: string;
}

export function listRuns(params: RunsListParams = {}): Promise<PageDto<RunDto>> {
  const qs = new URLSearchParams();
  if (params.limit !== undefined) qs.set("limit", String(params.limit));
  if (params.before !== undefined) qs.set("before", params.before);
  if (params.conversationId !== undefined) qs.set("conversation_id", params.conversationId);
  const suffix = qs.toString() ? `?${qs}` : "";
  return getJson<PageDto<RunDto>>(`/api/v1/runs${suffix}`);
}

export function getRun(id: string): Promise<RunDto> {
  return getJson<RunDto>(`/api/v1/runs/${encodeURIComponent(id)}`);
}

export function conversationEventsStreamUrl(id: string): string {
  return `/api/v1/conversations/${encodeURIComponent(id)}/events/stream`;
}
