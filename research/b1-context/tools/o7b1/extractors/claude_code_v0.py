"""o7-chat-export-claude-code-v0

Input:  a Claude Code session transcript (append-only JSONL; one record per line;
        record types include user, assistant, attachment, system, mode, ...).
Output: a canonical linear user-visible transcript (list of records).

The container transcript is already linear (append-only), so linearization is
just "records in file order". We keep only the user/assistant dialogue turns.

Inclusion policy (v0) — documented and returned in `policy`:
  INCLUDE  records of type user/assistant, isSidechain false, whose message
           content yields non-empty visible text. For list content, only `text`
           blocks are taken.
  EXCLUDE  every other record type (attachment/system/mode/queue-operation/
           last-prompt); isSidechain == true (subagent-internal turns);
           `thinking` blocks (chain-of-thought — never extracted); `tool_use`
           and `tool_result` blocks (tool plumbing, not user-authored dialogue
           text in v0); turns whose extracted visible text is empty.

Nothing is invented: role, id, parent, and timestamp are copied straight from
the record and left absent when the record omits them.
"""
from __future__ import annotations

import json

DERIVED_RECORD_SCHEMA = "o7.b1.derived-transcript/v0"


class ExtractorError(Exception):
    """Unknown/invalid required structure — fail closed."""


class ClaudeCodeV0:
    extractor_id = "o7-chat-export-claude-code-v0"
    version = "0"

    inclusion_policy = {
        "include_record_types": ["user", "assistant"],
        "linearization": "append-only JSONL file order",
        "visible_blocks": ["text", "(str content)"],
        "excluded": [
            "record types other than user/assistant",
            "isSidechain == true (subagent-internal turns)",
            "thinking blocks (chain-of-thought, never extracted)",
            "tool_use / tool_result blocks (tool plumbing)",
            "turns with empty extracted visible text",
        ],
        "chain_of_thought": "never extracted",
    }

    @staticmethod
    def _visible_text(content) -> str:
        if isinstance(content, str):
            return content
        if isinstance(content, list):
            texts: list[str] = []
            for b in content:
                if not isinstance(b, dict):
                    raise ExtractorError("content block is not an object")
                btype = b.get("type")
                if btype == "text":
                    t = b.get("text")
                    if isinstance(t, str) and t:
                        texts.append(t)
                elif btype in ("thinking", "tool_use", "tool_result"):
                    continue  # excluded by policy
                # unknown block types are ignored for text purposes but do not
                # crash: text extraction only claims what it recognizes.
            return "\n".join(texts)
        raise ExtractorError("unexpected message.content type: %r" % type(content).__name__)

    @classmethod
    def extract(cls, raw_bytes: bytes, *, source_id: str, raw_source_digest: str) -> tuple[list[dict], dict]:
        text = raw_bytes.decode("utf-8")
        lines = text.split("\n")
        records: list[dict] = []
        stats = {"records_total": 0, "dialogue_turns": 0, "emitted": 0, "sidechain_skipped": 0}
        seq = 0
        session_id = None
        for lineno, line in enumerate(lines):
            if not line.strip():
                continue
            try:
                r = json.loads(line)
            except json.JSONDecodeError as e:
                raise ExtractorError("invalid JSONL at line %d: %s" % (lineno, e))
            stats["records_total"] += 1
            if not isinstance(r, dict):
                continue
            if session_id is None:
                session_id = r.get("sessionId")
            rtype = r.get("type")
            if rtype not in ("user", "assistant"):
                continue
            stats["dialogue_turns"] += 1
            if r.get("isSidechain") is True:
                stats["sidechain_skipped"] += 1
                continue
            msg = r.get("message")
            if not isinstance(msg, dict):
                raise ExtractorError("dialogue record without message object at line %d" % lineno)
            role = msg.get("role")
            vtext = cls._visible_text(msg.get("content"))
            if not vtext.strip():
                continue
            rec = {
                "schema": DERIVED_RECORD_SCHEMA,
                "sequence": seq,
                "source_id": source_id,
                "session_id": r.get("sessionId", session_id),
                "native_message_or_event_id": r.get("uuid"),
                "parent_id": r.get("parentUuid"),
                "role": role,
                "event_kind": "message",
                "visible_text": vtext,
                "native_created_at": r.get("timestamp"),
                "source_pointer": "record/%s" % r.get("uuid"),
                "raw_source_digest": raw_source_digest,
            }
            records.append(rec)
            seq += 1
        stats["emitted"] = len(records)
        policy = {
            "extractor_id": cls.extractor_id,
            "extractor_version": cls.version,
            "inclusion_policy": cls.inclusion_policy,
            "stats": stats,
        }
        return records, policy
