#!/usr/bin/env python3
"""CLI entrypoint for the standalone o7.b1.evaluation/v1 structural evaluator.

Thin shell over `o7b1.evaluate_v1`. All logic (input closure, consistency gates,
structural gates, output assembly and self-validation) lives in the module so the
implementation closure is exactly the module + its reused helpers + the schemas.

Usage:
  evaluate_v1.py --contract CONTRACT.yaml \
    --input gold_state:gold-state.json=PATH \
    --input fixture_manifest:manifest.yaml=PATH \
    --input source_selectors:source-selectors.yaml=PATH \
    --input task_registry:tasks.yaml=PATH \
    --input budget:budget-v0.yaml=PATH \
    --input report:report.json=PATH \
    --input projection_comparison:projection-comparison.json=PATH \
    --input derived_manifest:derived-manifest.json=PATH \
    --input task_file:TASK_FILE=PATH \
    --input questions_file:QUESTIONS_FILE=PATH \
    --input task_context:TASK_ID=PATH \
    --input derived_body:SESSION_ID=PATH \
    --out evaluation-v1.json [--mode dev|qualification|authoritative]

Exit codes:
  0  a schema-valid evaluation-v1.json was emitted (overall may be PASS/FAIL/UNAVAILABLE)
  2  operational refusal (bad contract/identity/invocation/closure/output schema)
  3  internal evaluator exception
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from o7b1.evaluate_v1 import main  # noqa: E402

if __name__ == "__main__":
    raise SystemExit(main())
