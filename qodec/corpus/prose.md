# Why the harness records everything

A run that cannot be replayed is an anecdote, not evidence. The harness treats
every agent invocation as a small experiment: the task is the hypothesis, the
gate is the measurement, and the harvested record is the lab notebook. When a
verdict surprises us later, we do not argue about what the agent probably did;
we open the record and read what it actually did.

This discipline costs almost nothing at write time and pays for itself the
first time a regression appears two weeks after the fact. Diffs age well.
Memories do not. The record also keeps incentives honest: an agent that knows
its raw output is preserved has no room to summarize its own failures into
something flattering.

The second reason is budgeting. Token spend is invisible until it is measured,
and measurement changes behavior. Once each run carries its own cost line,
expensive habits stop hiding: the broad scans, the re-read files, the retries
that quietly triple a bill. None of this requires cleverness — only the
boring, adult practice of writing things down and being willing to be wrong in
public inside your own repository.
