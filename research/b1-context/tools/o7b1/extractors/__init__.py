"""Deterministic extractors: RAW platform capture -> canonical linear transcript.

Both extractors produce the same record shape (schema `o7.b1.derived-transcript/v0`)
so a derived transcript is comparable regardless of source platform. They are
strictly user-visible: hidden reasoning / chain-of-thought is never extracted or
reconstructed, backend metadata is never passed off as user-visible content, and
unknown *required* structure is a hard error (fail closed), never a guess.
"""
from .chatgpt_backend_api_v0 import ChatGPTBackendApiV0
from .claude_code_v0 import ClaudeCodeV0

DERIVED_RECORD_SCHEMA = "o7.b1.derived-transcript/v0"

EXTRACTORS = {
    ChatGPTBackendApiV0.extractor_id: ChatGPTBackendApiV0,
    ClaudeCodeV0.extractor_id: ClaudeCodeV0,
}

__all__ = ["ChatGPTBackendApiV0", "ClaudeCodeV0", "EXTRACTORS", "DERIVED_RECORD_SCHEMA"]
