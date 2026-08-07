/**
 * Independent relation oracle for the TypeScript fixtures.
 *
 * `tsc --noEmit` proves a fixture type-checks. It does NOT prove the caller set
 * is the one the oracle claims. This script closes that gap: it drives the
 * TypeScript language service and type checker to compute the relations
 * themselves, then compares them against each `oracle.yaml`.
 *
 * Exit codes
 *   0  every checkable fact agrees with the independent oracle
 *   1  at least one disagreement -- the corpus is wrong, or TypeScript is
 *   3  TypeScript unavailable; nothing was proved (never a silent pass)
 *
 * Relations
 *   callers          references to the seed, mapped to the enclosing function
 *   references       every reference, including non-call bindings such as a
 *                    renaming re-export
 *   implementations  getImplementationAtPosition on an interface member
 *
 * Overloads are handled through the checker rather than through references:
 * TypeScript groups all overload signatures of a name under one symbol, so
 * find-all-references cannot attribute a call to the signature it selected.
 * Where a seed names one signature among several, each call site is resolved
 * with getResolvedSignature and kept only if the selected declaration is the
 * seeded one.
 */
import { createRequire } from "node:module";
import { readFileSync, existsSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const require = createRequire(import.meta.url);

function loadTypeScript() {
  const candidates = [
    "typescript",
    "/opt/node22/lib/node_modules/typescript",
    "/usr/lib/node_modules/typescript",
    "/usr/local/lib/node_modules/typescript",
  ];
  for (const c of candidates) {
    try {
      return require(c);
    } catch {
      /* try next */
    }
  }
  return null;
}

const ts = loadTypeScript();
if (!ts) {
  console.log("  independent relation oracle: UNAVAILABLE (typescript not resolvable)");
  console.log("  -> relation claims in the oracles are UNPROVED, not proved.");
  process.exit(3);
}

// --- minimal YAML reader -----------------------------------------------------
// The oracles use a small, fixed subset. Rather than depend on a YAML package
// that may not be installed, shell out to the python3 that tools/check.sh
// already requires and read JSON back.
function loadOracle(file) {
  const { execFileSync } = require("node:child_process");
  const out = execFileSync(
    "python3",
    ["-c", "import sys,yaml,json;json.dump(yaml.safe_load(open(sys.argv[1])),sys.stdout)", file],
    { encoding: "utf8" },
  );
  return JSON.parse(out);
}

// --- language service over one fixture --------------------------------------
function serviceFor(files) {
  const options = {
    noEmit: true,
    strict: true,
    target: ts.ScriptTarget.ES2020,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.Bundler,
  };
  const versions = new Map(files.map((f) => [f, "1"]));
  const host = {
    getScriptFileNames: () => files,
    getScriptVersion: (f) => versions.get(f) ?? "1",
    getScriptSnapshot: (f) =>
      existsSync(f) ? ts.ScriptSnapshot.fromString(readFileSync(f, "utf8")) : undefined,
    getCurrentDirectory: () => process.cwd(),
    getCompilationSettings: () => options,
    getDefaultLibFileName: (o) => ts.getDefaultLibFilePath(o),
    fileExists: ts.sys.fileExists,
    readFile: ts.sys.readFile,
    readDirectory: ts.sys.readDirectory,
    directoryExists: ts.sys.directoryExists,
    getDirectories: ts.sys.getDirectories,
  };
  return ts.createLanguageService(host, ts.createDocumentRegistry());
}

const lineOf = (sf, pos) => sf.getLineAndCharacterOfPosition(pos).line + 1;

/** Locate the declaration a gold_identity names, by name and line range. */
function findSeedDeclaration(sf, symbolText, [lo, hi]) {
  const wanted = symbolText.split(" ")[0].split(".").pop();
  let hit = null;
  const visit = (node) => {
    const name = node.name;
    if (name && ts.isIdentifier(name) && name.text === wanted) {
      const line = lineOf(sf, name.getStart(sf));
      if (line >= lo && line <= hi && !hit) hit = node;
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
  return hit;
}

/** Nearest enclosing named function/method, for labelling a reference site. */
function enclosingSymbol(sf, pos) {
  let found = "<top-level>";
  const visit = (node) => {
    if (node.getStart(sf) <= pos && pos < node.getEnd()) {
      if (
        (ts.isFunctionDeclaration(node) ||
          ts.isMethodDeclaration(node) ||
          ts.isFunctionExpression(node)) &&
        node.name
      ) {
        found = node.name.getText(sf);
      }
      ts.forEachChild(node, visit);
    }
  };
  ts.forEachChild(sf, visit);
  return found;
}

/** All declarations of the symbol at a position (overload set included). */
function declarationsAt(checker, sf, node) {
  const sym = checker.getSymbolAtLocation(node.name ?? node);
  return sym?.declarations ?? [];
}

function collect(caseDir, oracle) {
  const files = [];
  const walk = (d) => {
    for (const e of require("node:fs").readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (p.endsWith(".ts")) files.push(p);
    }
  };
  walk(path.join(caseDir, "source"));

  const service = serviceFor(files);
  const program = service.getProgram();
  const checker = program.getTypeChecker();
  const results = [];

  for (const [i, fact] of oracle.facts.entries()) {
    const seed = oracle.seeds.find((s) => s.id === fact.seed);
    const seedFile = path.join(caseDir, seed.gold_identity.path);
    const sf = program.getSourceFile(seedFile);
    if (!sf) {
      results.push({ i, status: "SKIP", why: `seed file not in program: ${seed.gold_identity.path}` });
      continue;
    }
    const decl = findSeedDeclaration(sf, seed.gold_identity.symbol, seed.gold_identity.line_range);
    if (!decl) {
      results.push({ i, status: "SKIP", why: `seed declaration not located: ${seed.gold_identity.symbol}` });
      continue;
    }
    const pos = (decl.name ?? decl).getStart(sf);

    // Any expectation outside TypeScript (e.g. a contract file) is out of this
    // oracle's reach by construction -- report it, never silently drop it.
    const foreign = [...fact.expected, ...fact.forbidden].filter((e) => !e.path.endsWith(".ts"));

    let observed = [];
    if (fact.relation === "implementations") {
      const impls = service.getImplementationAtPosition(seedFile, pos) ?? [];
      observed = impls.map((d) => {
        const f = program.getSourceFile(d.fileName);
        return { path: path.relative(caseDir, d.fileName), line: lineOf(f, d.textSpan.start) };
      });
    } else {
      const groups = service.findReferences(seedFile, pos) ?? [];
      const decls = declarationsAt(checker, sf, decl);
      // A call never selects the implementation signature -- it selects one of
      // the overload signatures and reaches the implementation body through it.
      // Per-signature attribution therefore applies only when the SEED is
      // itself an overload signature (declared without a body). Seeding the
      // implementation asks a different question -- which calls reach this body
      // -- and its answer is every call to the symbol.
      const seedIsOverloadSignature = decls.length > 1 && !decl.body;
      for (const g of groups) {
        for (const ref of g.references) {
          if (ref.isDefinition) continue;
          const f = program.getSourceFile(ref.fileName);
          if (!f) continue;
          const node = ts.getTokenAtPosition(f, ref.textSpan.start);
          const call = ts.findAncestor(node, ts.isCallExpression);
          const isCallSite =
            !!call && call.expression.getStart(f) <= ref.textSpan.start &&
            ref.textSpan.start < call.expression.getEnd();

          if (fact.relation === "callers" && !isCallSite) continue;

          // Overload attribution: keep the call only if the signature it
          // actually selected is the seeded declaration.
          if (fact.relation === "callers" && seedIsOverloadSignature && isCallSite) {
            const sig = checker.getResolvedSignature(call);
            const sigDecl = sig?.declaration;
            if (sigDecl && sigDecl !== decl) continue;
          }

          observed.push({
            path: path.relative(caseDir, ref.fileName),
            line: lineOf(f, ref.textSpan.start),
            symbol: enclosingSymbol(f, ref.textSpan.start),
          });
        }
      }
    }

    const key = (e) => `${e.path}:${e.line}`;
    const seen = new Set(observed.map(key));
    const missing = fact.expected.filter((e) => e.path.endsWith(".ts") && !seen.has(key(e)));
    const present = fact.forbidden.filter((e) => e.path.endsWith(".ts") && seen.has(key(e)));

    results.push({
      i,
      relation: fact.relation,
      seed: fact.seed,
      status: missing.length || present.length ? "FAIL" : "PASS",
      missing,
      forbiddenPresent: present,
      foreign,
      observed,
    });
  }
  return results;
}

// --- run ---------------------------------------------------------------------
const root = process.cwd();
const fixtures = require("node:fs")
  .readdirSync(path.join(root, "fixtures"))
  .sort()
  .map((n) => path.join(root, "fixtures", n))
  .filter((d) => existsSync(path.join(d, "oracle.yaml")));

let failed = 0;
let checked = 0;
let skipped = 0;

for (const caseDir of fixtures) {
  const oracle = loadOracle(path.join(caseDir, "oracle.yaml"));
  if (oracle.language !== "typescript") {
    console.log(`  ${oracle.case}: SKIP (language=${oracle.language}, out of this oracle's reach)`);
    skipped++;
    continue;
  }
  for (const r of collect(caseDir, oracle)) {
    if (r.status === "SKIP") {
      console.log(`  ${oracle.case} fact ${r.i}: SKIP -- ${r.why}`);
      skipped++;
      continue;
    }
    checked++;
    const label = `${oracle.case} fact ${r.i} (${r.relation}, ${r.seed})`;
    if (r.status === "PASS") {
      const note = r.foreign.length
        ? ` [${r.foreign.length} non-TS expectation(s) out of reach: ${r.foreign.map((f) => f.path).join(", ")}]`
        : "";
      console.log(`  PASS  ${label}${note}`);
    } else {
      failed++;
      console.log(`  FAIL  ${label}`);
      for (const m of r.missing) console.log(`          expected but absent: ${m.path}:${m.line} (${m.symbol})`);
      for (const f of r.forbiddenPresent) console.log(`          forbidden but present: ${f.path}:${f.line} (${f.symbol})`);
      console.log(`          observed: ${r.observed.map((o) => `${o.path}:${o.line}`).join(", ") || "(none)"}`);
    }
  }
}

console.log(
  `\n  independent relation oracle: ${failed ? "FAIL" : "PASS"} ` +
    `(${checked} fact(s) checked, ${skipped} skipped, ${failed} disagreement(s))`,
);
process.exit(failed ? 1 : 0);
