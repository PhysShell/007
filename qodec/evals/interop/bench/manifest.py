"""Case manifests — an explicit pipeline of one producer and ordered transforms.

The design correction this increment makes: Graphify/CodeGraph are *producers*
(they create an artifact from a repo + query), not `optimize(text)` transforms.
RTK's stdin filters (`rtk log`) and qodec ARE transforms (text -> text). A case
states its pipeline in full, so an adapter can never silently ignore its input:

    {"id": "clap-derive",
     "producer": {"type": "codegraph", "repo": "clap", "query": "..."},
     "transforms": ["qodec"]}

    {"id": "build-log-rtk",
     "producer": {"type": "fixture", "path": "corpus/build-log.txt"},
     "transforms": [{"type": "rtk", "filter": "log"}, "qodec"]}

RTK command-runners (`rtk rg PATTERN PATH`) proxy a native command and cannot
transform arbitrary text, so they are a *producer* type (`rtk-command`) with a
`baseline` command for the raw reference — never a transform.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path

PRODUCER_TYPES = {"fixture", "command", "rtk-command", "codegraph"}
# Transform types the runner can actually execute today.
TRANSFORM_TYPES = {"rtk", "qodec"}
# Named but explicitly not executable (item 7) — recorded as unsupported arms.
UNSUPPORTED_TRANSFORMS = {"headroom", "fastcontext"}


@dataclass
class Producer:
    type: str
    raw: dict = field(default_factory=dict)


@dataclass
class Transform:
    type: str
    raw: dict = field(default_factory=dict)


@dataclass
class Case:
    id: str
    producer: Producer
    transforms: list[Transform]
    raw: dict = field(default_factory=dict)

    @property
    def arm(self) -> str:
        """The tool feeding qodec — how the case's qodec arm is named."""
        if self.producer.type == "rtk-command":
            return "rtk"
        if any(t.type == "rtk" for t in self.transforms):
            return "rtk"
        if self.producer.type == "codegraph":
            return "codegraph"
        return "raw"

    @property
    def unsupported(self) -> Transform | None:
        for t in self.transforms:
            if t.type in UNSUPPORTED_TRANSFORMS:
                return t
        return None


def _parse_transform(item: object) -> Transform:
    if isinstance(item, str):
        return Transform(type=item, raw={"type": item})
    if isinstance(item, dict) and "type" in item:
        return Transform(type=item["type"], raw=item)
    raise ValueError(f"bad transform entry: {item!r}")


def parse_case(obj: dict, *, stdin_filters: set[str]) -> Case:
    if "id" not in obj:
        raise ValueError("case missing 'id'")
    cid = obj["id"]
    p = obj.get("producer")
    if not isinstance(p, dict) or "type" not in p:
        raise ValueError(f"{cid}: producer must be an object with a 'type'")
    if p["type"] not in PRODUCER_TYPES:
        raise ValueError(f"{cid}: unknown producer type {p['type']!r}")
    producer = Producer(type=p["type"], raw=p)

    tlist = obj.get("transforms", [])
    transforms = [_parse_transform(t) for t in tlist]
    if not transforms or transforms[-1].type != "qodec":
        raise ValueError(f"{cid}: transforms must end with 'qodec'")
    for t in transforms[:-1]:
        if t.type == "qodec":
            raise ValueError(f"{cid}: only the terminal transform may be qodec")
        if t.type in UNSUPPORTED_TRANSFORMS:
            continue  # recorded as unsupported at run time, not a parse error
        if t.type not in TRANSFORM_TYPES:
            raise ValueError(f"{cid}: unknown transform type {t.type!r}")
        if t.type == "rtk":
            f = t.raw.get("filter")
            if f is None:
                raise ValueError(f"{cid}: rtk transform needs a 'filter'")
            # An rtk *transform* must be a real stdin filter — a command-runner
            # would ignore the incoming text, which the model forbids.
            if f not in stdin_filters:
                raise ValueError(
                    f"{cid}: rtk filter {f!r} is not a stdin filter "
                    f"(stdin filters: {sorted(stdin_filters)}); use an "
                    "rtk-command producer for command-runner filters"
                )

    # Producer-specific required fields.
    if producer.type == "fixture" and "path" not in p:
        raise ValueError(f"{cid}: fixture producer needs 'path'")
    if producer.type in ("command", "rtk-command") and "argv" not in p:
        raise ValueError(f"{cid}: {producer.type} producer needs 'argv'")
    if producer.type == "rtk-command" and "repo" not in p:
        raise ValueError(f"{cid}: rtk-command producer needs 'repo'")
    if producer.type == "codegraph" and ("repo" not in p or "query" not in p):
        raise ValueError(f"{cid}: codegraph producer needs 'repo' and 'query'")
    return Case(id=cid, producer=producer, transforms=transforms, raw=obj)


def load(path: Path, *, stdin_filters: set[str]) -> list[Case]:
    obj = json.loads(path.read_text())
    cases = obj.get("cases")
    if not isinstance(cases, list):
        raise ValueError(f"{path}: manifest must have a 'cases' array")
    return [parse_case(c, stdin_filters=stdin_filters) for c in cases]
