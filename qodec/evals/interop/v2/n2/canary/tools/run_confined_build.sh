#!/usr/bin/env bash
# Outer network-namespace + resource-limit wrapper around one Sandboy-confined
# command, for the N2-A miner canary.
#
# Layering (outermost to innermost):
#   ulimit (this shell)     -- address space / CPU time / process count caps,
#                              inherited across exec+fork regardless of privilege
#   sudo --preserve-env=... -- root, needed only to create the network
#                              namespace; explicitly allowlists the SAME small
#                              set of env names Sandboy's own policy allows
#                              (defense in depth, not reliance on sudo alone)
#   unshare --net           -- fresh network namespace: no interface but the
#                              loopback we bring up ourselves, i.e. no route
#                              to anywhere off-host
#   runuser -p -u <caller>  -- drop back to the unprivileged runner user,
#                              preserving the already-minimal environment
#   timeout                 -- wall-clock cap
#   sandboy run              -- Landlock + seccomp + its OWN env_clear()+
#                              env_allow confinement (final, authoritative)
#   <argv...>                -- the untrusted command
#
# Usage:
#   run_confined_build.sh <sandboy-bin> <policy.toml> <cwd> -- <argv...>
#
# The caller (capture_build.py) is responsible for exporting exactly the env
# names the Sandboy policy's env_allow lists, pointed at dedicated (non-host)
# values, before invoking this script.
set -euo pipefail

SANDBOY_BIN=$1; POLICY=$2; CWD=$3; shift 3
[ "${1:-}" = "--" ] || { echo "run_confined_build.sh: expected -- before argv" >&2; exit 2; }
shift

CALLER_USER=$(id -un)
PRESERVE_ENV="PATH,HOME,TMPDIR,DOTNET_ROOT,DOTNET_CLI_TELEMETRY_OPTOUT,DOTNET_NOLOGO,DOTNET_SKIP_FIRST_TIME_EXPERIENCE,DOTNET_MULTILEVEL_LOOKUP,DOTNET_GENERATE_ASPNET_CERTIFICATE"

ulimit -v $((4 * 1024 * 1024))  # 4 GiB address space
ulimit -t 600                    # 600s CPU time
ulimit -u 512                    # max processes/threads for this user

exec sudo --preserve-env="$PRESERVE_ENV" unshare --net bash -c '
  ip link set lo up
  cd "$1" || exit 1
  shift
  caller_user=$1; shift
  sandboy_bin=$1; shift
  policy=$1; shift
  exec runuser -p -u "$caller_user" -- timeout 900 "$sandboy_bin" run --policy "$policy" -- "$@"
' _ "$CWD" "$CALLER_USER" "$SANDBOY_BIN" "$POLICY" "$@"
