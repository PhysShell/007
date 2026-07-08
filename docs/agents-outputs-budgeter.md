Proposal: Agent Output Budgeter for AI Coding Agents

Status

Draft

Summary

This proposal introduces an Agent Output Budgeter: a local, deterministic middleware layer that reduces noisy command output before it enters an AI coding agent’s context window.

The goal is to make tools like Claude Code, Codex, MCP-based agents, and custom automation safer, cheaper, and more useful on large repositories. Instead of sending raw terminal output directly to the model, the system stores the full output locally and returns a compact, structured summary optimized for debugging and code navigation.

This is especially useful for large .NET / MSBuild / legacy repositories where build logs, test output, "rg" results, "git diff", and runtime logs can easily flood the model context with low-value text.

Problem

AI coding agents often execute shell commands and receive raw output directly into the model context.

Common examples:

- "rg SomeSymbol"
- "git diff"
- "git status"
- "dotnet build"
- "msbuild Broker.sln"
- "dotnet test"
- "cat large-file.log"
- "find ."
- "tree"
- "docker logs"
- SQL diagnostic output
- CI logs
- MSBuild binary logs

This creates several problems:

1. Context pollution
   Large outputs push useful context out of the model window.

2. Higher cost
   The model spends tokens on repeated warnings, irrelevant paths, generated files, dependency noise, stack trace spam, and duplicated diagnostics.

3. Worse reasoning
   More text does not mean better context. Raw output often hides the actual signal.

4. Poor debuggability
   Agents may miss the real error because it is buried under thousands of lines.

5. Security and trust concerns
   Closed-source “token optimizer” binaries that inspect command output are risky for private or commercial repositories.

6. Weak domain understanding
   Generic output compression does not understand project-specific build systems, legacy .NET Framework quirks, DevExpress dependencies, XAML build failures, binding redirects, or MSBuild diagnostic structure.

Goals

The system should:

- Reduce terminal output before it enters the agent context.
- Preserve full raw output locally for later inspection.
- Return concise, structured, deterministic summaries.
- Support project-specific reducers for .NET, MSBuild, tests, Git, search, logs, and SQL.
- Work with multiple agent frontends:
  - Claude Code hooks
  - Codex wrappers
  - MCP tools
  - local CLI usage
  - CI/report processing
- Avoid external services.
- Avoid telemetry by default.
- Avoid closed-source binaries.
- Be safe enough for private and commercial repositories.
- Make failures easier to debug, not merely shorter.

Non-goals

This proposal does not aim to:

- Replace semantic code navigation tools.
- Replace full repository packers.
- Replace source control.
- Replace CI.
- Hide important errors from the user.
- Use an LLM to summarize every command output.
- Send command output to a remote service.
- Depend on a proprietary binary.
- Optimize prompts by vague “AI magic”.

This system should be boring, local, explainable, and predictable.

Motivation

Existing tools already show that output reduction is useful:

- CLI wrappers can reduce noisy command output.
- Repository packers can prepare selected source files for AI consumption.
- MCP tools can expose semantic project operations to agents.
- Claude Code hooks can intercept tool calls before and after execution.

However, general-purpose tools are not enough for large legacy repositories. A serious .NET/MSBuild codebase needs reducers that understand:

- MSBuild diagnostics
- ".binlog" files
- failed projects
- task/target chains
- "ResolveAssemblyReference"
- NuGet/package restore errors
- XAML compilation errors
- generated files
- flaky tests
- repeated warnings
- legacy ".NET Framework 4.7.2" constraints
- DevExpress version mismatches
- binding redirects
- CI-specific environment problems

A generic “shorten stdout” tool helps, but a domain-aware reducer helps much more.

Proposed Solution

Introduce a local tool called Agent Output Budgeter.

Working name:

aob

Alternative names:

agent-budget
agent-output
context-saver
stdout-budgeter
safe-exec

The tool wraps command execution and returns a compact result to the agent.

High-level Flow

AI Agent
  ↓
safe_exec / aob run / MCP tool / Claude hook
  ↓
Command execution
  ↓
Raw output captured
  ↓
Raw output saved to local artifact store
  ↓
Command-specific reducer runs
  ↓
Structured compact summary returned to agent
  ↓
Agent may request full artifact only when needed

Example

Instead of giving the agent this:

10,000 lines of MSBuild output
800 repeated warnings
NuGet restore noise
Generated file paths
Verbose target logs
One real compiler error hidden near the end

The agent receives this:

Command: msbuild Broker.sln /p:Configuration=Release /p:Platform="Any CPU"
Exit code: 1
Duration: 02:14

Summary:
- Build failed in 2 projects.
- 1 compiler error.
- 37 warnings, 5 unique warning types.
- Full raw output saved to: .agent-artifacts/2026-07-06/183012-msbuild/output.log

Errors:
1. Login.Admin/LoginViewModel.cs:142
   CS1061: 'UserSession' does not contain a definition for 'Token'

2. Login.STS.Identity/Login.csproj
   ResolveAssemblyReference failed:
   Could not resolve DevExpress.Xpf.Core.v12.2

Likely next steps:
- Inspect LoginViewModel.cs around line 142.
- Verify DevExpress reference paths.
- Check package/reference mismatch for DevExpress 12.2 vs newer restored packages.

The full log remains available, but the model does not eat it by default like a raccoon in a landfill.

Architecture

src/
  AgentOutputBudgeter/
    Cli/
      aob
    Core/
      CommandClassifier
      ExecutionEngine
      OutputCapture
      ArtifactStore
      BudgetPolicy
      ReducerPipeline
      SummaryFormat
    Reducers/
      GitReducer
      RipgrepReducer
      FileListingReducer
      DotNetBuildReducer
      MSBuildReducer
      DotNetTestReducer
      TrxReducer
      LogReducer
      JsonReducer
      SqlReducer
      GenericReducer
    Integrations/
      ClaudeCodeHooks
      CodexWrapper
      McpServer
      GitHubActions
    Config/
      aob.config.json

Core Components

1. Command Classifier

Detects what kind of command is being executed.

Examples:

Command| Reducer
"git status"| "GitStatusReducer"
"git diff"| "GitDiffReducer"
"rg Foo"| "RipgrepReducer"
"dotnet build"| "DotNetBuildReducer"
"msbuild *.sln"| "MSBuildReducer"
"dotnet test"| "DotNetTestReducer"
"cat *.log"| "LogReducer"
"docker logs"| "LogReducer"
unknown command| "GenericReducer"

The classifier should be rule-based first. No LLM is needed.

2. Execution Engine

Runs commands safely and captures:

- command
- working directory
- stdout
- stderr
- exit code
- duration
- environment summary
- timestamp
- truncated preview
- full raw artifact path

The execution engine should support:

- timeout
- max output size
- kill process tree
- working directory restrictions
- environment allowlist/blocklist
- command denylist for dangerous operations, if used as an agent tool

3. Artifact Store

Stores full outputs locally.

Suggested structure:

.agent-artifacts/
  2026-07-06/
    183012-msbuild/
      command.txt
      stdout.log
      stderr.log
      summary.md
      metadata.json
      diagnostics.json

The agent receives paths and artifact IDs, not the full raw output.

4. Reducer Pipeline

A reducer takes raw output and produces a compact summary.

Reducer output should include:

- command
- exit code
- duration
- artifact path
- high-level summary
- diagnostics
- omitted output count
- recommended next inspection points
- confidence level
- reducer name/version

Example reducer output:

{
  "command": "dotnet test Login.Tests.csproj",
  "exitCode": 1,
  "durationMs": 43122,
  "artifactId": "2026-07-06/183012-dotnet-test",
  "summary": {
    "status": "failed",
    "totalTests": 312,
    "passed": 309,
    "failed": 3,
    "skipped": 0
  },
  "diagnostics": [
    {
      "kind": "test_failure",
      "name": "LoginTests.RejectsExpiredToken",
      "message": "Expected Unauthorized but got OK",
      "file": "Login.Tests/Auth/LoginTests.cs",
      "line": 88
    }
  ]
}

5. Budget Policy

Controls how much output may enter agent context.

Example config:

{
  "defaultMaxChars": 12000,
  "defaultMaxDiagnostics": 50,
  "storeRawOutput": true,
  "artifactDirectory": ".agent-artifacts",
  "reducers": {
    "git.diff": {
      "maxFiles": 30,
      "maxHunksPerFile": 5,
      "includeGeneratedFiles": false
    },
    "ripgrep": {
      "maxMatches": 80,
      "maxMatchesPerFile": 5
    },
    "msbuild": {
      "preferBinlog": true,
      "maxWarnings": 25,
      "collapseRepeatedWarnings": true
    },
    "dotnet.test": {
      "showPassedTests": false,
      "maxStackFrames": 8
    }
  }
}

Reducers

Git Status Reducer

Should return:

- branch
- staged files
- unstaged files
- untracked files
- deleted files
- renamed files
- conflict state
- short summary

Avoid dumping the full porcelain output when it is large.

Git Diff Reducer

Should return:

- changed file list
- file status
- stats
- important hunks
- changed symbols if detectable
- generated files omitted
- lock files summarized separately
- binary files listed, not dumped

Example:

Changed files: 12
- 4 C# files
- 2 test files
- 1 project file
- 1 package lock file
- 4 generated files omitted

Important hunks:
- LoginViewModel.cs: token refresh logic changed
- AuthService.cs: new null-check added
- Login.Tests.cs: added expired-token regression test

Ripgrep Reducer

Should return:

- number of files matched
- number of total matches
- top matching files
- limited matches per file
- surrounding lines only when useful
- generated/vendor paths omitted by default

Example:

Search: rg "StationId"
Matched: 47 lines in 12 files

Top files:
1. Licensing/StationIdProvider.cs - 11 matches
2. Licensing/StationFingerprint.cs - 8 matches
3. Tests/StationIdTests.cs - 7 matches

Omitted:
- 21 additional matches
- 3 generated files

File Listing Reducer

For commands like:

- "ls"
- "find"
- "tree"
- "dir"

Should return:

- directory count
- file count
- top-level structure
- likely project roots
- omitted directories
- ignored build artifacts

Should hide by default:

- "bin/"
- "obj/"
- ".git/"
- "node_modules/"
- "packages/"
- ".vs/"
- generated caches

DotNet Build Reducer

Should detect:

- failed projects
- compiler errors
- warnings
- package restore errors
- target framework errors
- SDK mismatch
- nullable warnings
- analyzer warnings
- generated code warnings

Should group diagnostics by:

- project
- file
- diagnostic code
- severity
- repeated message

Example:

Build failed.

Projects:
- Login.Admin: failed
- Login.Api: succeeded
- Login.STS.Identity: failed

Errors:
1. Login.Admin/ViewModels/LoginViewModel.cs:142
   CS1061: 'UserSession' does not contain a definition for 'Token'

2. Login.STS.Identity/Login.STS.Identity.csproj
   NETSDK1045: Current .NET SDK does not support targeting .NET 8.0

Warnings:
- CS8618: 14 occurrences
- NU1701: 3 occurrences

MSBuild Reducer

This is a critical reducer for legacy .NET repositories.

It should support:

- plain MSBuild output
- ".binlog" parsing
- project failure summary
- target/task chain
- "ResolveAssemblyReference" diagnostics
- NuGet restore diagnostics
- XAML compilation errors
- binding redirect warnings
- reference conflicts
- package version mismatches

Preferred mode:

msbuild Broker.sln /bl:.agent-artifacts/latest/build.binlog

Then parse the ".binlog" using a structured parser.

The reducer should return:

- failed nodes
- failed projects
- first real error per project
- repeated warnings collapsed
- suspicious reference resolution issues
- path to ".binlog"
- path to structured diagnostics JSON

DotNet Test Reducer

Should return:

- total tests
- passed
- failed
- skipped
- failed test names
- assertion messages
- relevant stack frames
- test duration
- flaky-looking failures if repeated runs are available

Should not dump every passed test.

TRX Reducer

If ".trx" files are available, prefer them over parsing console output.

Should return:

- test run metadata
- failed tests
- error messages
- stack traces limited to project frames
- attachments
- result file path

Log Reducer

For runtime logs, service logs, and CI logs.

Should return:

- time range
- error count
- warning count
- exception groups
- repeated messages
- first occurrence
- last occurrence
- top stack traces
- correlation IDs if present

Should group repeated errors aggressively.

JSON / JSONL Reducer

Should return:

- schema sample
- key frequency
- number of records
- error-looking fields
- top repeated values
- first N relevant records
- omitted count

SQL Reducer

Should return:

- query duration
- affected rows
- result row count
- schema preview
- top rows only
- execution errors
- deadlock/timeouts if detected
- suspicious full-table scans if plan data is available

Agent Integrations

Claude Code Integration

Use hooks:

- "PreToolUse"
- "PostToolUse"

Possible behavior:

1. Rewrite selected shell commands to use "aob run".
2. After command execution, reduce output before it reaches the model.
3. Store full output in ".agent-artifacts".

Example:

git diff

becomes:

aob run -- git diff

Codex Integration

Possible strategies:

1. Use wrapper commands:
   
   - "aob rg"
   - "aob git diff"
   - "aob dotnet build"

2. Use an MCP tool:
   
   - "run_command_budgeted"

3. Use a shell proxy:
   
   - expose "safe_exec" instead of raw shell access

4. Use project-level instructions:
   
   - tell Codex to prefer "aob run -- <command>" for noisy commands

MCP Integration

Expose tools:

run_command_budgeted
read_artifact
list_artifacts
read_diagnostics
rerun_last_command
parse_msbuild_binlog
parse_trx

Example MCP tool:

{
  "name": "run_command_budgeted",
  "arguments": {
    "command": "dotnet test Login.Tests.csproj",
    "timeoutSeconds": 300,
    "maxSummaryChars": 12000
  }
}

CI Integration

The same reducers can run in CI.

Example:

aob reduce --kind msbuild --input build.log --binlog build.binlog --output summary.md

This can produce:

- PR comments
- GitHub Actions summaries
- Azure DevOps build summaries
- local diagnostic artifacts

Security Model

The system must be safe by design.

Requirements:

- Local execution only.
- No telemetry by default.
- No remote API calls for reduction.
- No proprietary binary requirement.
- Configurable denylist for dangerous commands.
- Raw outputs stay local.
- Secrets redaction before summary generation.
- Artifact directory can be gitignored.
- Clear separation between raw output and model-visible summary.

Default ".gitignore" entry:

.agent-artifacts/

Secret Redaction

Before returning anything to the agent, the system should redact common secrets:

- API keys
- bearer tokens
- connection strings
- passwords
- private keys
- NuGet PATs
- Azure DevOps PATs
- JWTs
- cookies
- authorization headers

Redaction should happen before reducer output is shown.

Example:

Authorization: Bearer [REDACTED]
Password=[REDACTED]

Error Handling

The reducer must never hide failure.

If reduction fails, return a safe fallback:

Command failed or reducer crashed.

Exit code: 1
Raw output saved to:
.agent-artifacts/2026-07-06/183012/output.log

Reducer error:
MSBuildReducer failed to parse binlog: invalid format

Fallback preview:
<first 200 lines>
<last 200 lines>

Failure of the reducer must not destroy the raw command output.

WHERE'S THE ERROR HANDLING?! It belongs here, not in a postmortem after the agent confidently ignores the only real compiler error.

Configuration

Example "aob.config.json":

{
  "artifactDirectory": ".agent-artifacts",
  "storeRawOutput": true,
  "redactSecrets": true,
  "defaultTimeoutSeconds": 300,
  "defaultMaxSummaryChars": 12000,
  "commands": {
    "deny": [
      "rm -rf /",
      "format",
      "shutdown"
    ]
  },
  "paths": {
    "ignore": [
      "bin/",
      "obj/",
      ".git/",
      ".vs/",
      "node_modules/",
      "packages/",
      "**/*.Designer.cs",
      "**/*.g.cs"
    ]
  },
  "reducers": {
    "git.diff": {
      "maxFiles": 40,
      "maxHunksPerFile": 5,
      "ignoreLockFiles": false
    },
    "ripgrep": {
      "maxFiles": 30,
      "maxMatchesPerFile": 5
    },
    "msbuild": {
      "preferBinlog": true,
      "maxErrors": 50,
      "maxWarnings": 30,
      "collapseRepeatedWarnings": true
    },
    "dotnet.test": {
      "showPassedTests": false,
      "maxFailedTests": 50,
      "maxStackFrames": 8
    }
  }
}

CLI Design

Run a command

aob run -- dotnet build

Run MSBuild with artifact capture

aob run -- msbuild Broker.sln /p:Configuration=Release /p:Platform="Any CPU" /bl

Reduce existing output

aob reduce --kind msbuild --input build.log

Parse binary log

aob msbuild summarize --binlog build.binlog

Parse TRX

aob test summarize --trx TestResults/result.trx

Show artifact

aob artifact show 2026-07-06/183012-msbuild

Print last summary

aob last

Output Format

The CLI should support:

- human-readable Markdown
- JSON
- compact plain text

Example:

aob run --format markdown -- dotnet test
aob run --format json -- dotnet test

JSON is useful for MCP and CI integrations.

Markdown is useful for agent context and PR comments.

Implementation Plan

Phase 1: Minimal CLI

Implement:

- "aob run -- <command>"
- output capture
- artifact store
- generic reducer
- secret redaction
- config loading

Acceptance criteria:

- Full output is stored.
- Agent-visible output is capped.
- Exit code is preserved.
- Timeout works.
- Reducer failure does not lose raw output.

Phase 2: Git and Search Reducers

Implement:

- "git status"
- "git diff"
- "rg"
- "grep"
- "find"
- "tree"

Acceptance criteria:

- Large diffs are summarized by file and hunk.
- Search results are grouped by file.
- Generated/build folders are omitted by default.
- Omitted counts are visible.

Phase 3: .NET Build Reducers

Implement:

- "dotnet build"
- "msbuild"
- compiler diagnostic parser
- warning grouping
- failed project detection
- SDK/framework mismatch detection
- NuGet/package restore error detection

Acceptance criteria:

- Build failures show exact project/file/line/code.
- Repeated warnings are collapsed.
- Failed projects are listed before warnings.
- Summary is useful without raw log.

Phase 4: MSBuild Binlog Support

Implement:

- ".binlog" detection
- structured binlog parsing
- target/task failure chain
- reference resolution diagnostics
- DevExpress/reference mismatch hints where possible

Acceptance criteria:

- Binlog path is preserved.
- Failed target/task chain is shown.
- "ResolveAssemblyReference" problems are summarized.
- Legacy .NET Framework build issues are easier to inspect.

Phase 5: Test Reducers

Implement:

- "dotnet test"
- TRX parsing
- failed tests
- stack trace trimming
- assertion extraction

Acceptance criteria:

- Failed tests are shown first.
- Passed tests are not dumped.
- Stack traces are limited to relevant project frames.
- TRX is preferred over console parsing when available.

Phase 6: Agent Integrations

Implement:

- Claude Code hook scripts
- MCP server
- Codex wrapper instructions
- project-level setup command

Acceptance criteria:

- Claude Code can use reducers automatically.
- Codex can use "aob run" or MCP.
- MCP exposes artifact reading and command execution.
- Setup is reversible.

Phase 7: CI Integration

Implement:

- GitHub Actions summary output
- Azure DevOps summary output
- PR comment mode
- artifact upload metadata

Acceptance criteria:

- CI can publish compact build/test summaries.
- Full logs remain available as artifacts.
- PR comments contain only useful diagnostics.

Project-specific Extensions

For this repository, add custom reducers for known pain points:

Legacy .NET Framework / WPF

Detect and summarize:

- XAML compilation errors
- binding redirect warnings
- missing assemblies
- old DevExpress references
- designer/generated file noise
- AnyCPU/x86/x64 mismatch
- App.config transform issues

Azure DevOps / NuGet

Detect and summarize:

- 401/403 package restore errors
- wrong NuGet source
- expired PAT
- package source priority issues
- "packages.config" vs "PackageReference" mismatch
- unexpected package version upgrades

SQL / Database Logs

Detect and summarize:

- timeout errors
- deadlocks
- failed migrations
- long-running queries
- connection string redaction
- row count explosions

Comparison With Existing Tools

Generic CLI output reducers

Useful for common commands, but usually lack deep project-specific understanding.

Good for:

- "git"
- "rg"
- "ls"
- "tree"
- common test runners

Not enough for:

- legacy MSBuild diagnostics
- ".binlog"
- DevExpress reference failures
- old .NET Framework quirks
- project-specific CI failures

Repository packers

Useful for preparing source context, but they do not solve noisy command output.

Good for:

- static code review
- one-time repository context
- prompt preparation

Not enough for:

- live build/test/debug loops
- iterative agent command execution
- CI diagnostics

Semantic MCP code tools

Useful for code navigation and editing.

Good for:

- symbol search
- reference lookup
- controlled edits
- code-aware workflows

Not enough for:

- raw command output reduction
- build log summarization
- test failure grouping

Closed-source token optimizers

Not recommended for private or commercial repositories unless there is a strong trust model.

Problems:

- closed binary sees command output
- difficult to audit
- unclear telemetry behavior
- licensing ambiguity
- weak project-specific customization

Risks

Risk: Important output may be hidden

Mitigation:

- always store raw output
- show artifact path
- show omitted counts
- preserve exit code
- allow "--no-reduce"
- allow "aob artifact show"

Risk: Reducer produces misleading summary

Mitigation:

- deterministic parsers first
- avoid LLM-based summarization by default
- include reducer confidence
- include raw artifact path
- test reducers against real logs

Risk: Security leaks

Mitigation:

- secret redaction
- local-only processing
- no telemetry by default
- artifact gitignore
- configurable redaction patterns

Risk: Too much custom logic

Mitigation:

- start with small reducers
- keep reducers composable
- use config for project-specific rules
- add tests from real logs

Acceptance Criteria

The project is successful if:

1. Agents receive significantly smaller command outputs.
2. Full raw output is never lost.
3. Build/test failures become easier to diagnose.
4. Common repository commands become cheaper and cleaner.
5. No closed-source dependency is required.
6. No remote service is required.
7. The system works with Claude Code, Codex, and MCP-style agents.
8. Legacy .NET/MSBuild diagnostics are better than generic stdout truncation.

Example Agent Instruction

Add this to project-level AI instructions:

When running commands that may produce large output, prefer:

aob run -- <command>

Use this especially for:
- git diff
- rg / grep
- dotnet build
- msbuild
- dotnet test
- docker logs
- large cat/type commands

Do not request full raw output unless the reduced summary is insufficient.
If needed, inspect the artifact path returned by aob.

Example ".gitignore"

# Agent Output Budgeter artifacts
.agent-artifacts/

Open Questions

1. Should the first implementation be in Rust or .NET?
2. Should MCP support be built into the main binary or shipped as a separate adapter?
3. Should MSBuild ".binlog" parsing be implemented immediately or after plain log parsing?
4. Should reducers be plugin-based from the beginning?
5. Should project-specific reducers live in this repository or in separate packages?
6. Should CI summaries be Markdown-only or also SARIF/JUnit-compatible?
7. Should "aob run" enforce command safety, or should it only reduce output?

Recommendation

Start with a small local CLI and the highest-value reducers:

1. "aob run"
2. artifact store
3. secret redaction
4. "git diff"
5. "rg"
6. "dotnet build"
7. "msbuild"
8. "dotnet test"

After that, add:

1. ".binlog" parser
2. TRX parser
3. MCP integration
4. Claude Code hooks
5. CI summary mode

This gives immediate value without depending on proprietary tools or vague token-optimizer magic.

Final Position

The right approach is not to trust a closed-source binary with repository command output.

The right approach is to build a local, deterministic, auditable output budgeter that understands the project’s actual development workflow.

Generic reducers handle common command noise.

Project-specific reducers handle the real pain.

That combination should produce better agent behavior, lower token usage, clearer debugging, and a safer trust model.