"""Safe capture engine: run argv arrays with an explicit environment allowlist.

NEVER uses a shell. Child processes get a freshly built environment (canonical
locale/timezone + an isolated HOME + only the allowlisted names actually present
in the parent), never the inherited runner environment. Credential-bearing
variables are stripped even if allowlisted.
"""
from __future__ import annotations

import os
import subprocess
import time
from pathlib import Path

# Interpreters that would (re)introduce shell semantics — forbidden in N0.
SHELL_INTERPRETERS = {"bash", "sh", "zsh", "dash", "ksh", "fish", "cmd", "cmd.exe",
                      "powershell", "powershell.exe", "pwsh"}
SHELL_C_FLAGS = {"-c", "/c", "/C", "-Command", "-EncodedCommand"}
SHELL_METACHARS = ("|", "&&", "||", ";", ">", "<", "`", "$(", ")")

# Never forward these to child processes (exact names or substring markers).
FORBIDDEN_ENV_EXACT = {
    "SSH_AUTH_SOCK", "GITHUB_TOKEN", "GH_TOKEN",
    "AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_SESSION_TOKEN",
    "GOOGLE_APPLICATION_CREDENTIALS", "OPENAI_API_KEY", "ANTHROPIC_API_KEY",
    "HTTPS_PROXY", "HTTP_PROXY", "https_proxy", "http_proxy",
}
FORBIDDEN_ENV_MARKERS = ("TOKEN", "SECRET", "PASSWORD", "CREDENTIAL", "APIKEY", "API_KEY")


class CaptureError(Exception):
    pass


def env_name_is_forbidden(name: str) -> bool:
    if name in FORBIDDEN_ENV_EXACT:
        return True
    up = name.upper()
    return any(m in up for m in FORBIDDEN_ENV_MARKERS)


def assert_argv_no_shell(argv: list[str]) -> None:
    if not argv:
        raise CaptureError("empty argv")
    exe = Path(argv[0]).name
    if exe in SHELL_INTERPRETERS:
        raise CaptureError(f"shell interpreter '{argv[0]}' is forbidden")
    for a in argv:
        if a in SHELL_C_FLAGS:
            raise CaptureError(f"shell command flag '{a}' is forbidden")
        for meta in SHELL_METACHARS:
            if meta in a:
                raise CaptureError(f"shell metacharacter {meta!r} in argv token {a!r} is forbidden")


def safe_join(base: Path, rel: str) -> Path:
    """Resolve `rel` under `base`, refusing absolute paths, '..' escapes and
    symlink escapes."""
    if os.path.isabs(rel):
        raise CaptureError(f"absolute path not allowed: {rel}")
    if ".." in Path(rel).parts:
        raise CaptureError(f"parent-directory traversal not allowed: {rel}")
    base_r = base.resolve()
    target = (base_r / rel).resolve()
    if base_r != target and base_r not in target.parents:
        raise CaptureError(f"path escapes bundle: {rel}")
    return target


def build_child_env(allowlist: list[str], recipe: dict, home_dir: str) -> dict[str, str]:
    env = {
        "LC_ALL": recipe["locale"],
        "LANG": recipe["locale"],
        "TZ": recipe["timezone"],
        "SOURCE_DATE_EPOCH": str(recipe["source_date_epoch"]),
        "HOME": home_dir,
    }
    for name in allowlist:
        if name in ("HOME", "LC_ALL", "LANG", "TZ", "SOURCE_DATE_EPOCH"):
            continue  # canonical values win
        if env_name_is_forbidden(name):
            continue  # never forward credentials
        if name in os.environ:
            env[name] = os.environ[name]
    return env


def run_step(bundle_dir: Path, argv: list[str], cwd: str, stdin_path: str | None,
             env: dict[str, str], timeout_s: float, stdin_bytes: bytes | None = None,
             record_argv: list[str] | None = None) -> dict:
    """Execute `argv` (already resolved to real binaries). `record_argv` is the
    canonical recipe argv stored in the receipt so receipts stay portable across
    environments; shell-safety is checked against it."""
    assert_argv_no_shell(record_argv or argv)
    work = safe_join(bundle_dir, cwd) if cwd not in (".", "", None) else bundle_dir.resolve()
    if stdin_bytes is None:
        stdin_bytes = b""
        if stdin_path:
            with open(safe_join(bundle_dir, stdin_path), "rb") as fh:
                stdin_bytes = fh.read()
    t0 = time.monotonic()
    timed_out = False
    try:
        p = subprocess.run(
            argv, cwd=str(work), env=env, input=stdin_bytes,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout_s,
        )
        stdout, stderr, code = p.stdout, p.stderr, p.returncode
    except subprocess.TimeoutExpired as e:
        stdout = e.stdout or b""
        stderr = e.stderr or b""
        code = -1
        timed_out = True
    except FileNotFoundError as e:
        raise CaptureError(f"executable not found (undeclared tool?): {argv[0]} ({e})")
    wall = time.monotonic() - t0
    return {
        "argv": list(record_argv or argv), "cwd": cwd, "stdin_bytes": stdin_bytes,
        "stdout": stdout, "stderr": stderr, "exit_code": code,
        "wall_time_s": round(wall, 6), "timed_out": timed_out,
    }


def exit_code_matches(code: int, klass: str, expected: int | None) -> bool:
    if klass == "any":
        return True
    if klass == "zero":
        return code == 0
    if klass == "nonzero":
        return code != 0
    if klass == "exact":
        return expected is not None and code == expected
    return False
