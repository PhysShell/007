"""o7-chat-export-chatgpt-backend-api-v0

Input:  a ChatGPT backend-api conversation JSON (a `mapping` graph of nodes, each
        node = {id, parent, children, message}, plus a `current_node` pointer).
Output: a canonical linear user-visible transcript (list of records).

Linearization
-------------
The conversation graph can contain edit branches. The *visible* thread is the
single path from the root to `current_node`, walked via `parent` pointers and
then reversed. Off-path branches (superseded edits) are deliberately dropped —
they are not what the user was looking at at capture time.

Inclusion policy (v0) — documented and returned in `policy`:
  INCLUDE  author.role in {user, assistant} whose content is user-visible text
           (content_type in {text, multimodal_text}); for multimodal, only the
           string parts are taken and any non-text parts are counted as omitted.
  EXCLUDE  system messages; any message with
           metadata.is_visually_hidden_from_conversation truthy; hidden-reasoning
           content types {thoughts, reasoning_recap}; tool-directed assistant
           messages (recipient set and != "all") and author.role == tool; any
           non-dialogue content type (code, execution_output, tether_*, etc.);
           messages whose extracted visible text is empty.

Nothing here reconstructs missing roles, timestamps, or identities: a field is
emitted only when the source actually carries it.
"""
from __future__ import annotations

from typing import Any

DERIVED_RECORD_SCHEMA = "o7.b1.derived-transcript/v0"

_HIDDEN_REASONING_CONTENT_TYPES = frozenset({"thoughts", "reasoning_recap"})
_VISIBLE_TEXT_CONTENT_TYPES = frozenset({"text", "multimodal_text"})


class ExtractorError(Exception):
    """Unknown/invalid required structure — fail closed."""


class ChatGPTBackendApiV0:
    extractor_id = "o7-chat-export-chatgpt-backend-api-v0"
    version = "0"

    inclusion_policy = {
        "include_roles": ["user", "assistant"],
        "visible_content_types": sorted(_VISIBLE_TEXT_CONTENT_TYPES),
        "linearization": "root..current_node active path via parent pointers",
        "excluded": [
            "author.role == system",
            "author.role == tool",
            "metadata.is_visually_hidden_from_conversation truthy",
            "hidden-reasoning content types: %s" % sorted(_HIDDEN_REASONING_CONTENT_TYPES),
            "tool-directed assistant messages (recipient set and != 'all')",
            "non-dialogue content types (code / execution_output / tether_* / ...)",
            "messages with empty extracted visible text",
        ],
        "chain_of_thought": "never extracted",
    }

    @staticmethod
    def _active_path(mapping: dict, current_node: str | None) -> list[str]:
        if current_node is None or current_node not in mapping:
            # Fall back to the unique childless leaf reachable from a root, but only
            # if unambiguous; otherwise fail closed rather than guess a thread.
            leaves = [nid for nid, n in mapping.items() if not n.get("children")]
            if len(leaves) != 1:
                raise ExtractorError(
                    "no usable current_node and %d candidate leaves" % len(leaves)
                )
            current_node = leaves[0]
        path: list[str] = []
        seen: set[str] = set()
        nid: str | None = current_node
        while nid is not None:
            if nid in seen:
                raise ExtractorError("cycle in mapping parent chain at %s" % nid)
            seen.add(nid)
            path.append(nid)
            node = mapping.get(nid)
            if node is None:
                raise ExtractorError("dangling parent pointer to %s" % nid)
            nid = node.get("parent")
        path.reverse()
        return path

    @staticmethod
    def _visible_text(content: dict) -> tuple[str, int]:
        """Return (text, non_text_parts_omitted)."""
        parts = content.get("parts")
        if parts is None:
            return "", 0
        if not isinstance(parts, list):
            raise ExtractorError("content.parts is not a list")
        texts: list[str] = []
        omitted = 0
        for p in parts:
            if isinstance(p, str):
                if p:
                    texts.append(p)
            else:
                omitted += 1
        return "\n".join(texts), omitted

    @classmethod
    def extract(cls, raw: dict, *, source_id: str, raw_source_digest: str) -> tuple[list[dict], dict]:
        if not isinstance(raw, dict) or "mapping" not in raw:
            raise ExtractorError("not a ChatGPT backend-api conversation (no mapping)")
        mapping = raw["mapping"]
        if not isinstance(mapping, dict):
            raise ExtractorError("mapping is not an object")
        session_id = raw.get("conversation_id")
        path = cls._active_path(mapping, raw.get("current_node"))

        records: list[dict] = []
        stats = {"nodes_on_path": len(path), "emitted": 0, "non_text_parts_omitted": 0}
        seq = 0
        for nid in path:
            node = mapping[nid]
            msg = node.get("message")
            if not msg:
                continue
            author = msg.get("author") or {}
            role = author.get("role")
            if role in (None, "system", "tool"):
                continue
            meta = msg.get("metadata") or {}
            if meta.get("is_visually_hidden_from_conversation"):
                continue
            recipient = msg.get("recipient")
            if recipient not in (None, "all"):
                continue  # tool-directed
            content = msg.get("content") or {}
            ctype = content.get("content_type")
            if ctype in _HIDDEN_REASONING_CONTENT_TYPES:
                continue  # never extract chain-of-thought
            if ctype not in _VISIBLE_TEXT_CONTENT_TYPES:
                continue  # non-dialogue payload
            text, omitted = cls._visible_text(content)
            stats["non_text_parts_omitted"] += omitted
            if not text.strip():
                continue
            parent_id = None
            parent_nid = node.get("parent")
            if parent_nid is not None:
                pnode = mapping.get(parent_nid) or {}
                pmsg = pnode.get("message")
                if pmsg:
                    parent_id = pmsg.get("id")
            rec = {
                "schema": DERIVED_RECORD_SCHEMA,
                "sequence": seq,
                "source_id": source_id,
                "session_id": session_id,
                "native_message_or_event_id": msg.get("id"),
                "parent_id": parent_id,
                "role": role,
                "event_kind": "message",
                "visible_text": text,
                "native_created_at": msg.get("create_time"),
                "source_pointer": "mapping/%s" % nid,
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
