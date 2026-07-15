#!/usr/bin/env bash
# Outer network-namespace + resource-limit wrapper around one Sandboy-confined
# command, for the N2-A miner canary.
#
# Layering (outermost to innermost):
#   ulimit (this shell)     -- CPU time / process count caps, inherited across
#                              exec+fork regardless of privilege. NOT address
#                              space (RLIMIT_AS): CoreCLR reserves virtual
#                              memory far beyond what it commits/uses at
#                              startup, and a `ulimit -v` cap makes it fail
#                              immediately with HRESULT 0x8007000E
#                              (E_OUTOFMEMORY) — confirmed against the real
#                              canary build, not a hypothetical. This is a
#                              genuine interop constraint, not something to
#                              force through; wall-clock (`timeout`) and CPU
#                              time/process-count limits below don't have
#                              this problem.
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
# `sudo` strips almost every environment variable unless named here --
# real N2-D1b evidence (CI run #7) showed RUSTUP_TOOLCHAIN and VIRTUAL_ENV
# (both correctly set by the caller and correctly listed in the Sandboy
# policy's own env_allow) produce byte-identical failures to before those
# fixes existed, because this list -- written only for N2-A's dotnet-only
# canary -- silently dropped them at the `sudo` layer, before Sandboy's own
# confinement ever saw them. This is now the union of every env_allow name
# across all 5 N2-D1b ecosystems (generic_sandbox_policy.py's
# ECOSYSTEM_POLICY_HINTS), not just dotnet's -- a strict superset of the
# original dotnet-only list, so N2-A's own (frozen, already-accepted) usage
# is unaffected.
PRESERVE_ENV="PATH,HOME,TMPDIR,DOTNET_ROOT,DOTNET_CLI_TELEMETRY_OPTOUT,DOTNET_NOLOGO,DOTNET_SKIP_FIRST_TIME_EXPERIENCE,DOTNET_MULTILEVEL_LOOKUP,DOTNET_GENERATE_ASPNET_CERTIFICATE,NUGET_PACKAGES,CARGO_HOME,RUSTUP_HOME,CARGO_NET_OFFLINE,RUSTUP_TOOLCHAIN,PYTHONDONTWRITEBYTECODE,PIP_NO_INDEX,VIRTUAL_ENV,JAVA_HOME,MAVEN_OPTS,GRADLE_USER_HOME,GRADLE_OPTS"

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
