# tasks/ — gold localization + patch tasks (Level 3)

Empty until the agent rung is built. Each task is derived from an
already-fixed PR/commit so scoring never depends on anyone's architectural
interpretation:

1. take 10–20 already-fixed PRs/commits from a corpus repo;
2. `git checkout` the parent commit;
3. `task` = the original issue / description;
4. `gold` changed files/symbols = the actual commit's diff;
5. score localization **Recall@k** first;
6. then apply the agent's patch and run tests.

One JSON file per task: `{"id", "repo", "parent_sha", "task", "gold_files":
[...], "gold_symbols": [...]}`.
