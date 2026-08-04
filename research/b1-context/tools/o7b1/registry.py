"""Single fixture task registry `o7.b1.fixture-tasks/v0`.

Before this module, `project_case.py` and `run_case.py` each carried their own
hardcoded `TASKS = [...]` list while claiming they could never disagree about
which tasks exist. They could. The task set now lives in ONE committed file per
fixture (`tasks-v0.yaml`), both CLIs load it here, and adding a future task or a
holdout fixture never touches Python source.

Everything is fail-closed. A registry that cannot be interpreted exactly — an
unknown field, a duplicate task file or task id, a missing referenced file, a
questions file whose `task_id` disagrees with its task, an absolute path, a
`..`, or any path escaping the fixture directory — raises `RegistryError`
instead of being approximated.
"""
from __future__ import annotations

import os

import yaml

from .canonical import sha256_of_file

REGISTRY_SCHEMA = "o7.b1.fixture-tasks/v0"
REGISTRY_FILENAME = "tasks-v0.yaml"

_ALLOWED_TOP_KEYS = frozenset({"schema", "fixture_id", "tasks"})
_ALLOWED_ENTRY_KEYS = frozenset({"task_file", "questions_file"})


class RegistryError(Exception):
    """The fixture task registry cannot be interpreted exactly."""


def _load_yaml(path: str):
    with open(path, encoding="utf-8") as fh:
        return yaml.safe_load(fh)


def _safe_fixture_path(fixture_dir: str, name: str) -> str:
    """Resolve a registry-referenced filename inside the fixture dir, or fail.

    Only a bare filename directly inside the fixture directory is allowed: no
    absolute paths, no `..`, no directory separators, nothing that resolves
    outside the fixture directory.
    """
    if not isinstance(name, str) or not name:
        raise RegistryError("registry path must be a non-empty string, got %r" % (name,))
    if os.path.isabs(name):
        raise RegistryError("absolute paths are not allowed in the registry: %r" % name)
    if name != os.path.basename(name) or os.sep in name or (os.altsep and os.altsep in name):
        raise RegistryError("registry path must be a bare filename, got %r" % name)
    if name in (".", "..") or ".." in name.split("."):
        raise RegistryError("registry path may not contain '..': %r" % name)
    full = os.path.normpath(os.path.join(fixture_dir, name))
    real_fixture = os.path.realpath(fixture_dir)
    real_full = os.path.realpath(full)
    if os.path.commonpath([real_fixture, real_full]) != real_fixture:
        raise RegistryError("registry path escapes the fixture directory: %r" % name)
    if not os.path.isfile(full):
        raise RegistryError("registry references a missing file: %r" % name)
    return full


def load_task_registry(fixture_dir: str, fixture_id: str) -> dict:
    """Load and fully validate the fixture task registry.

    Returns {"registry_path", "registry_digest", "entries": [
        {"task_file", "questions_file", "task", "questions"} ... ]} in the
    canonical order the registry lists them.
    """
    registry_path = os.path.join(fixture_dir, REGISTRY_FILENAME)
    if not os.path.isfile(registry_path):
        raise RegistryError("no fixture task registry at %s" % registry_path)
    reg = _load_yaml(registry_path)
    if not isinstance(reg, dict):
        raise RegistryError("registry must be a mapping")
    unknown = sorted(set(reg) - _ALLOWED_TOP_KEYS)
    if unknown:
        raise RegistryError("unknown registry field(s): %s" % unknown)
    if reg.get("schema") != REGISTRY_SCHEMA:
        raise RegistryError("registry schema must be %r, got %r"
                            % (REGISTRY_SCHEMA, reg.get("schema")))
    if reg.get("fixture_id") != fixture_id:
        raise RegistryError("registry fixture_id %r != requested %r"
                            % (reg.get("fixture_id"), fixture_id))
    tasks = reg.get("tasks")
    if not isinstance(tasks, list) or not tasks:
        raise RegistryError("registry `tasks` must be a non-empty list")

    entries = []
    seen_task_files: set[str] = set()
    seen_questions_files: set[str] = set()
    seen_task_ids: set[str] = set()
    for i, entry in enumerate(tasks):
        if not isinstance(entry, dict):
            raise RegistryError("registry tasks[%d] must be a mapping" % i)
        unk = sorted(set(entry) - _ALLOWED_ENTRY_KEYS)
        if unk:
            raise RegistryError("registry tasks[%d] has unknown field(s): %s" % (i, unk))
        task_file = entry.get("task_file")
        questions_file = entry.get("questions_file")
        if not task_file or not questions_file:
            raise RegistryError("registry tasks[%d] needs task_file and questions_file" % i)
        if task_file in seen_task_files:
            raise RegistryError("duplicate task_file in registry: %r" % task_file)
        if questions_file in seen_questions_files:
            raise RegistryError("duplicate questions_file in registry: %r" % questions_file)
        seen_task_files.add(task_file)
        seen_questions_files.add(questions_file)

        task_path = _safe_fixture_path(fixture_dir, task_file)
        questions_path = _safe_fixture_path(fixture_dir, questions_file)
        task = _load_yaml(task_path)
        questions = _load_yaml(questions_path)
        if not isinstance(task, dict) or not task.get("task_id"):
            raise RegistryError("%s has no task_id" % task_file)
        if not isinstance(questions, dict) or not questions.get("task_id"):
            raise RegistryError("%s has no task_id" % questions_file)
        if questions["task_id"] != task["task_id"]:
            raise RegistryError(
                "%s declares task_id %r but %s is %r"
                % (questions_file, questions["task_id"], task_file, task["task_id"]))
        if task["task_id"] in seen_task_ids:
            raise RegistryError("duplicate task_id in registry: %r" % task["task_id"])
        seen_task_ids.add(task["task_id"])
        entries.append({
            "task_file": task_file,
            "questions_file": questions_file,
            "task": task,
            "questions": questions,
        })

    return {
        "registry_path": registry_path,
        "registry_digest": "sha256:" + sha256_of_file(registry_path),
        "entries": entries,
    }
