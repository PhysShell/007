"""A tiny, dependency-free JSON-Schema subset validator.

Supports the keywords the corpus schemas use: type (string or list), enum,
pattern, minLength, minimum, required, properties, additionalProperties (bool),
items, minItems, uniqueItems, and local $ref ("#/definitions/...") with a
top-level `definitions` block. Returns a list of human-readable error strings.
"""
from __future__ import annotations

import json
import re


def _type_ok(value, typ: str) -> bool:
    if typ == "object":
        return isinstance(value, dict)
    if typ == "array":
        return isinstance(value, list)
    if typ == "string":
        return isinstance(value, str)
    if typ == "integer":
        return isinstance(value, int) and not isinstance(value, bool)
    if typ == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if typ == "boolean":
        return isinstance(value, bool)
    if typ == "null":
        return value is None
    return True


def _resolve(schema: dict, root: dict):
    ref = schema.get("$ref")
    if not ref:
        return schema
    if not ref.startswith("#/"):
        raise ValueError(f"unsupported $ref {ref!r}")
    node = root
    for part in ref[2:].split("/"):
        node = node[part]
    return node


def validate(instance, schema: dict, root: dict | None = None, path: str = "") -> list[str]:
    root = root if root is not None else schema
    schema = _resolve(schema, root)
    errs: list[str] = []
    here = path or "<root>"

    if "type" in schema:
        types = schema["type"]
        types = types if isinstance(types, list) else [types]
        if not any(_type_ok(instance, t) for t in types):
            errs.append(f"{here}: expected type {schema['type']}")
            return errs

    if "enum" in schema and instance not in schema["enum"]:
        errs.append(f"{here}: {instance!r} not in enum {schema['enum']}")

    if isinstance(instance, str):
        if "pattern" in schema and not re.search(schema["pattern"], instance):
            errs.append(f"{here}: {instance!r} fails pattern {schema['pattern']}")
        if "minLength" in schema and len(instance) < schema["minLength"]:
            errs.append(f"{here}: shorter than minLength {schema['minLength']}")

    if isinstance(instance, (int, float)) and not isinstance(instance, bool):
        if "minimum" in schema and instance < schema["minimum"]:
            errs.append(f"{here}: below minimum {schema['minimum']}")

    if isinstance(instance, dict):
        for req in schema.get("required", []):
            if req not in instance:
                errs.append(f"{here}: missing required field '{req}'")
        props = schema.get("properties", {})
        if schema.get("additionalProperties") is False:
            for key in instance:
                if key not in props:
                    errs.append(f"{here}: additional property '{key}' not allowed")
        for key, sub in props.items():
            if key in instance:
                errs.extend(validate(instance[key], sub, root, f"{path}.{key}" if path else key))

    if isinstance(instance, list):
        if "minItems" in schema and len(instance) < schema["minItems"]:
            errs.append(f"{here}: fewer than minItems {schema['minItems']}")
        if schema.get("uniqueItems"):
            seen = {json.dumps(x, sort_keys=True) for x in instance}
            if len(seen) != len(instance):
                errs.append(f"{here}: items not unique")
        item_schema = schema.get("items")
        if item_schema:
            for i, item in enumerate(instance):
                errs.extend(validate(item, item_schema, root, f"{path}[{i}]"))

    return errs
