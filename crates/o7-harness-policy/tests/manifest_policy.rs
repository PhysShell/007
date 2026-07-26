//! Fail-closed repository gate for o7-worker's `test-harness` feature.
//!
//! STRUCTURAL analysis (a real TOML parse, never a substring grep) of every `Cargo.toml` in the
//! workspace tree. It is REPOSITORY-GLOBAL: it first reads the workspace root's
//! `[workspace.dependencies]` into a key→package map, so it resolves EFFECTIVE package identity
//! whether the rename is local (`package = "..."`) OR inherited by a member via `workspace = true`
//! (whose identity lives only in the root). An aliased edge — direct or inherited — cannot hide,
//! and the feature is detected being turned on through ANY route:
//! - a dependency edge (any of `dependencies` / `dev-dependencies` / `build-dependencies`, every
//!   `target.*.*dependencies`, `workspace.dependencies`) whose effective package is o7-worker and
//!   whose `features` list includes `test-harness`;
//! - a `[features]` forward to an o7-worker alias — `alias/test-harness` or the weak
//!   `alias?/test-harness` — under the alias's effective identity, not a literal name;
//! - a local self-activation `feature = ["test-harness"]` inside o7-worker's own manifest.
//!
//! Exactly ONE enablement is sanctioned, checked by full identity — o7-worker's own
//! `[dev-dependencies]` self-edge with dependency key `o7-worker`, effective package `o7-worker`,
//! and `path = "."`. Any other enablement (a production/build edge, a renamed alias, a forward, a
//! duplicate self-alias, a wrong path) fails the gate, and the total number of sanctioned
//! enablements must be exactly one. A second gate pins the harness BINARY lock (autobins off,
//! every harness bin `required-features`-gated, no un-classified `src/bin`). Traversal and parsing are
//! fail-closed: any unreadable directory/entry, unreadable/malformed manifest, or path error is a
//! hard failure, never a silent "all clean".
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const FEATURE: &str = "test-harness";
const PKG: &str = "o7-worker";
const ALLOWED_MANIFEST: &str = "crates/o7-worker/Cargo.toml";
const ALLOWED_TABLE: &str = "dev-dependencies";
const HARNESS_BINS: [&str; 4] = ["sandboy_fake", "spawn_probe", "fd_probe", "sandbox_probe"];

// ---------------------------------------------------------------------------
// Fail-closed filesystem traversal.
// ---------------------------------------------------------------------------

/// Walk UP to the directory whose `Cargo.toml` declares `[workspace]`. An EXISTING but
/// unreadable/malformed manifest on the way up is a hard error, never silently skipped.
fn workspace_root() -> Result<PathBuf, String> {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let manifest = dir.join("Cargo.toml");
        match std::fs::read_to_string(&manifest) {
            Ok(text) => {
                let table: toml::Table = text
                    .parse()
                    .map_err(|e| format!("malformed {}: {e}", manifest.display()))?;
                if table.contains_key("workspace") {
                    return Ok(dir);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("unreadable {}: {e}", manifest.display())),
        }
        if !dir.pop() {
            return Err("no [workspace] Cargo.toml found above CARGO_MANIFEST_DIR".to_owned());
        }
    }
}

/// Every `Cargo.toml` under `root` (skipping only `target/` and `.git/`), fail-closed: a directory
/// that cannot be enumerated, or an entry/metadata error, aborts the scan rather than under-report.
fn all_manifests(root: &Path) -> Result<Vec<PathBuf>, String> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
        let entries = std::fs::read_dir(dir)
            .map_err(|e| format!("cannot enumerate {}: {e}", dir.display()))?;
        for entry in entries {
            let entry = entry.map_err(|e| format!("dir entry error in {}: {e}", dir.display()))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|e| format!("file-type error on {}: {e}", path.display()))?;
            if file_type.is_dir() {
                let name = entry.file_name();
                if name != "target" && name != ".git" {
                    walk(&path, out)?;
                }
            } else if file_type.is_file() && entry.file_name() == "Cargo.toml" {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(root, &mut out)?;
    out.sort();
    Ok(out)
}

// ---------------------------------------------------------------------------
// Structural manifest analysis (pure — the adversarial fixtures call it directly).
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Enablement {
    detail: String,
    sanctioned: bool,
}

/// The `[workspace.dependencies]` map (dependency key → effective package) parsed from the
/// workspace ROOT manifest. A member's `workspace = true` edge carries no `package` of its own, so
/// its identity lives here — the analysis must be repository-global, not per-manifest.
fn workspace_dependency_map(root_text: &str) -> Result<BTreeMap<String, String>, String> {
    let root: toml::Table = root_text
        .parse()
        .map_err(|e| format!("parse workspace root: {e}"))?;
    let mut map = BTreeMap::new();
    if let Some(deps) = root
        .get("workspace")
        .and_then(toml::Value::as_table)
        .and_then(|ws| ws.get("dependencies"))
        .and_then(toml::Value::as_table)
    {
        for (key, item) in deps {
            let pkg = item
                .as_table()
                .and_then(|t| t.get("package"))
                .and_then(toml::Value::as_str)
                .unwrap_or(key);
            map.insert(key.clone(), pkg.to_owned());
        }
    }
    Ok(map)
}

/// Whether a dependency item inherits from the workspace (`workspace = true`).
fn is_workspace_inherited(item: &toml::Value) -> bool {
    item.as_table()
        .and_then(|t| t.get("workspace"))
        .and_then(toml::Value::as_bool)
        == Some(true)
}

/// Effective package of a dependency `key = item`, resolving a local `package = "..."` rename AND
/// an inherited `workspace = true` alias (whose identity is in the workspace root map). Fail-closed
/// if `workspace = true` names a key absent from that map (a Cargo-invalid state we must not pass).
fn effective_package(
    key: &str,
    item: &toml::Value,
    ws: &BTreeMap<String, String>,
) -> Result<String, String> {
    if let Some(pkg) = item
        .as_table()
        .and_then(|t| t.get("package"))
        .and_then(toml::Value::as_str)
    {
        return Ok(pkg.to_owned());
    }
    if is_workspace_inherited(item) {
        return ws.get(key).cloned().ok_or_else(|| {
            format!(
                "dependency `{key}` inherits `workspace = true` but no \
                 [workspace.dependencies] `{key}` exists"
            )
        });
    }
    Ok(key.to_owned())
}

fn dep_has_feature(item: &toml::Value, feature: &str) -> bool {
    item.as_table()
        .and_then(|t| t.get("features"))
        .and_then(toml::Value::as_array)
        .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(feature)))
}

fn dep_path(item: &toml::Value) -> Option<&str> {
    item.as_table()
        .and_then(|t| t.get("path"))
        .and_then(toml::Value::as_str)
}

/// `(label, table)` for every dependency table, including `target.<cfg>.{,dev-,build-}dependencies`
/// and `workspace.dependencies`.
fn dependency_tables(root: &toml::Table) -> Vec<(String, &toml::Table)> {
    let mut out = Vec::new();
    for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(t) = root.get(kind).and_then(toml::Value::as_table) {
            out.push((kind.to_owned(), t));
        }
    }
    if let Some(ws) = root.get("workspace").and_then(toml::Value::as_table) {
        if let Some(t) = ws.get("dependencies").and_then(toml::Value::as_table) {
            out.push(("workspace.dependencies".to_owned(), t));
        }
    }
    if let Some(targets) = root.get("target").and_then(toml::Value::as_table) {
        for (cfg, spec) in targets {
            let Some(spec) = spec.as_table() else {
                continue;
            };
            for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
                if let Some(t) = spec.get(kind).and_then(toml::Value::as_table) {
                    out.push((format!("target.{cfg}.{kind}"), t));
                }
            }
        }
    }
    out
}

/// Every dependency KEY in the manifest whose EFFECTIVE package is o7-worker (across all tables),
/// resolving inherited `workspace = true` aliases via `ws`.
fn o7_worker_aliases(
    root: &toml::Table,
    ws: &BTreeMap<String, String>,
) -> Result<BTreeSet<String>, String> {
    let mut set = BTreeSet::new();
    for (_label, table) in dependency_tables(root) {
        for (key, item) in table {
            if effective_package(key, item, ws)? == PKG {
                set.insert(key.clone());
            }
        }
    }
    Ok(set)
}

/// All the ways one manifest turns `test-harness` on, each tagged sanctioned/forbidden. `ws` is the
/// workspace-root dependency map, so inherited (`workspace = true`) o7-worker aliases are resolved.
fn analyze(
    rel: &str,
    text: &str,
    ws: &BTreeMap<String, String>,
) -> Result<Vec<Enablement>, String> {
    let root: toml::Table = text.parse().map_err(|e| format!("parse {rel}: {e}"))?;
    let aliases = o7_worker_aliases(&root, ws)?;
    let mut out = Vec::new();

    // Dependency edges (effective package o7-worker) enabling the feature.
    for (label, table) in dependency_tables(&root) {
        for (key, item) in table {
            if effective_package(key, item, ws)? != PKG || !dep_has_feature(item, FEATURE) {
                continue;
            }
            let path = dep_path(item);
            let sanctioned = rel == ALLOWED_MANIFEST
                && label == ALLOWED_TABLE
                && key == PKG
                && path == Some(".");
            out.push(Enablement {
                detail: format!(
                    "{rel} :: [{label}] `{key}` (package={PKG}, path={path:?}) features += \"{FEATURE}\""
                ),
                sanctioned,
            });
        }
    }

    // Feature forwarding: `alias/test-harness`, weak `alias?/test-harness`, or (in o7-worker's own
    // manifest) a bare `test-harness` self-activation.
    if let Some(features) = root.get("features").and_then(toml::Value::as_table) {
        for (fname, arr) in features {
            let Some(list) = arr.as_array() else { continue };
            for entry in list {
                let Some(s) = entry.as_str() else { continue };
                match s.split_once('/') {
                    Some((left, right)) => {
                        let alias = left.strip_suffix('?').unwrap_or(left);
                        if right == FEATURE && aliases.contains(alias) {
                            out.push(Enablement {
                                detail: format!(
                                    "{rel} :: feature \"{fname}\" forwards \"{s}\" (o7-worker alias `{alias}`)"
                                ),
                                sanctioned: false,
                            });
                        }
                    }
                    None => {
                        if s == FEATURE && rel == ALLOWED_MANIFEST {
                            out.push(Enablement {
                                detail: format!(
                                    "{rel} :: feature \"{fname}\" self-activates \"{FEATURE}\""
                                ),
                                sanctioned: false,
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}

fn rel_of(manifest: &Path, root: &Path) -> Result<String, String> {
    manifest
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .map_err(|e| format!("relative path of {}: {e}", manifest.display()))
}

// ---------------------------------------------------------------------------
// Binary-lock policy (pure — fixtures call it directly).
// ---------------------------------------------------------------------------

/// `bin_files` are the `*.rs` stems present in `crates/o7-worker/src/bin`.
fn binary_policy(text: &str, bin_files: &BTreeSet<String>) -> Result<(), String> {
    let root: toml::Table = text.parse().map_err(|e| format!("parse manifest: {e}"))?;

    let autobins = root
        .get("package")
        .and_then(toml::Value::as_table)
        .and_then(|p| p.get("autobins"))
        .and_then(toml::Value::as_bool);
    if autobins != Some(false) {
        return Err(format!(
            "package.autobins must be `false` (found {autobins:?}); auto-discovery could add an \
             un-gated harness binary"
        ));
    }

    let mut declared: BTreeMap<String, bool> = BTreeMap::new();
    let bins = root
        .get("bin")
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();
    for bin in &bins {
        let name = bin
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or("a [[bin]] entry has no name")?;
        let gated = bin
            .get("required-features")
            .and_then(toml::Value::as_array)
            .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(FEATURE)));
        declared.insert(name.to_owned(), gated);
    }

    for (name, gated) in &declared {
        if !gated {
            return Err(format!(
                "bin `{name}` is not gated by required-features = [\"{FEATURE}\"]"
            ));
        }
    }
    let declared_names: BTreeSet<&str> = declared.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = HARNESS_BINS.into_iter().collect();
    if declared_names != expected {
        return Err(format!(
            "declared bins {declared_names:?} != the sanctioned harness bins {expected:?}"
        ));
    }
    // Every source under src/bin must be one of the classified harness bins (fail-closed even with
    // autobins off: an un-classified source must never sit there waiting for autobins to flip).
    for stem in bin_files {
        if !declared.contains_key(stem) {
            return Err(format!(
                "src/bin/{stem}.rs is not a classified harness binary"
            ));
        }
    }
    Ok(())
}

fn bin_stems(bin_dir: &Path) -> Result<BTreeSet<String>, String> {
    let mut stems = BTreeSet::new();
    let entries = match std::fs::read_dir(bin_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(stems),
        Err(e) => return Err(format!("cannot enumerate {}: {e}", bin_dir.display())),
    };
    for entry in entries {
        let entry = entry.map_err(|e| format!("bin dir entry error: {e}"))?;
        let path = entry.path();
        if path.extension().and_then(|x| x.to_str()) == Some("rs") {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("non-UTF-8 bin stem")?;
            stems.insert(stem.to_owned());
        }
    }
    Ok(stems)
}

// ---------------------------------------------------------------------------
// The real gate over the actual workspace.
// ---------------------------------------------------------------------------

#[test]
fn test_harness_enablement_is_exactly_the_sanctioned_self_edge() {
    let root = workspace_root().expect("workspace root");
    let manifests = all_manifests(&root).expect("fail-closed manifest walk");
    // Repository-global: resolve inherited (`workspace = true`) aliases against the root map.
    let root_text =
        std::fs::read_to_string(root.join("Cargo.toml")).expect("read workspace root manifest");
    let ws = workspace_dependency_map(&root_text).expect("workspace dependency map");

    let mut sanctioned = Vec::new();
    let mut violations = Vec::new();
    for manifest in &manifests {
        let rel = rel_of(manifest, &root).expect("relative path");
        let text = std::fs::read_to_string(manifest).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        for enablement in analyze(&rel, &text, &ws).unwrap_or_else(|e| panic!("{e}")) {
            if enablement.sanctioned {
                sanctioned.push(enablement.detail);
            } else {
                violations.push(enablement.detail);
            }
        }
    }

    assert!(
        violations.is_empty(),
        "REPOSITORY POLICY VIOLATION — `{FEATURE}` may be enabled only from the o7-worker self \
         dev-dependency ({ALLOWED_MANIFEST} [{ALLOWED_TABLE}]); forbidden enablement(s):\n{}",
        violations.join("\n")
    );
    assert_eq!(
        sanctioned.len(),
        1,
        "there must be EXACTLY one sanctioned `{FEATURE}` enablement; found {}: {sanctioned:?}",
        sanctioned.len()
    );
}

#[test]
fn production_feature_graph_excludes_the_test_harness_feature() {
    // Production edges only (`no-dev`): the o7-worker node must resolve WITHOUT `test-harness`,
    // proving the feature is reachable solely through the dev self-edge. Cargo's own structural
    // feature resolution, complementing the manifest analysis above.
    let root = workspace_root().expect("workspace root");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = std::process::Command::new(cargo)
        .args([
            "tree",
            "--workspace",
            "--locked",
            "--edges",
            "no-dev,features",
            "--prefix",
            "none",
        ])
        .current_dir(&root)
        .output()
        .expect("cargo tree runs");
    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let line = line.trim();
        assert!(
            !(line.contains(PKG) && line.contains(FEATURE)),
            "production feature graph activates `{FEATURE}` on `{PKG}`: {line:?}"
        );
    }
}

#[test]
fn harness_binaries_are_locked_out_of_production() {
    let root = workspace_root().expect("workspace root");
    let manifest = root.join(ALLOWED_MANIFEST);
    let text = std::fs::read_to_string(&manifest).expect("read o7-worker manifest");
    let stems = bin_stems(&root.join("crates/o7-worker/src/bin")).expect("enumerate src/bin");
    binary_policy(&text, &stems).expect("harness binary lock");
}

// ---------------------------------------------------------------------------
// Adversarial fixtures — every known bypass must be caught.
// ---------------------------------------------------------------------------

/// The fixture, analysed as if it were a PRODUCTION manifest (no workspace inheritance), yields ≥1
/// enablement and none is sanctioned — i.e. the real gate would reject it.
fn assert_all_forbidden(rel: &str, text: &str) {
    assert_forbidden_with(&BTreeMap::new(), rel, text);
}

/// As [`assert_all_forbidden`], but with an explicit workspace-dependency map so a member manifest
/// that INHERITS a renamed o7-worker alias (`workspace = true`) is analysed as Cargo resolves it.
fn assert_forbidden_with(ws: &BTreeMap<String, String>, rel: &str, text: &str) {
    let found = analyze(rel, text, ws).expect("fixture parses");
    assert!(
        !found.is_empty(),
        "gate MISSED an enablement in fixture:\n{text}"
    );
    assert!(
        found.iter().all(|e| !e.sanctioned),
        "gate wrongly sanctioned an enablement in fixture:\n{found:#?}"
    );
}

const PROD: &str = "crates/o7-verifier/Cargo.toml";

/// A workspace root that renames o7-worker as `worker-internal` in `[workspace.dependencies]`.
const ROOT_RENAME: &str = r#"
[workspace]
members = ["crates/o7-worker", "crates/o7-verifier"]

[workspace.dependencies.worker-internal]
package = "o7-worker"
path = "crates/o7-worker"
"#;

#[test]
fn adversarial_renamed_production_dependency() {
    assert_all_forbidden(
        PROD,
        r#"
[dependencies.worker-internal]
package = "o7-worker"
path = "../o7-worker"
features = ["test-harness"]
"#,
    );
}

#[test]
fn adversarial_renamed_target_specific_dependency() {
    assert_all_forbidden(
        PROD,
        r#"
[target.'cfg(windows)'.dependencies.worker-internal]
package = "o7-worker"
path = "../o7-worker"
features = ["test-harness"]
"#,
    );
}

#[test]
fn adversarial_alias_feature_forwarding() {
    assert_all_forbidden(
        PROD,
        r#"
[dependencies.worker-internal]
package = "o7-worker"
path = "../o7-worker"
optional = true

[features]
production = ["worker-internal/test-harness"]
"#,
    );
}

#[test]
fn adversarial_weak_alias_feature_forwarding() {
    assert_all_forbidden(
        PROD,
        r#"
[dependencies.worker-internal]
package = "o7-worker"
path = "../o7-worker"
optional = true

[features]
weak-production = ["worker-internal?/test-harness"]
"#,
    );
}

#[test]
fn adversarial_local_self_activation_in_o7_worker_manifest() {
    // A feature in o7-worker's OWN manifest that pulls in `test-harness` by default.
    assert_all_forbidden(
        ALLOWED_MANIFEST,
        r#"
[features]
test-harness = []
default = ["test-harness"]
"#,
    );
}

// ---- inherited workspace-alias fixtures (whole-tree: root map + member text) ----

#[test]
fn adversarial_inherited_workspace_alias_production_edge() {
    let ws = workspace_dependency_map(ROOT_RENAME).expect("root parses");
    assert_forbidden_with(
        &ws,
        PROD,
        r#"
[dependencies]
worker-internal = { workspace = true, features = ["test-harness"] }
"#,
    );
}

#[test]
fn adversarial_inherited_workspace_alias_target_specific_edge() {
    let ws = workspace_dependency_map(ROOT_RENAME).expect("root parses");
    assert_forbidden_with(
        &ws,
        PROD,
        r#"
[target.'cfg(windows)'.dependencies]
worker-internal = { workspace = true, features = ["test-harness"] }
"#,
    );
}

#[test]
fn adversarial_inherited_workspace_alias_forward() {
    let ws = workspace_dependency_map(ROOT_RENAME).expect("root parses");
    assert_forbidden_with(
        &ws,
        PROD,
        r#"
[dependencies]
worker-internal = { workspace = true, optional = true }

[features]
prod = ["worker-internal/test-harness"]
"#,
    );
}

#[test]
fn adversarial_inherited_workspace_alias_weak_forward() {
    let ws = workspace_dependency_map(ROOT_RENAME).expect("root parses");
    assert_forbidden_with(
        &ws,
        PROD,
        r#"
[dependencies]
worker-internal = { workspace = true, optional = true }

[features]
weak-prod = ["worker-internal?/test-harness"]
"#,
    );
}

#[test]
fn fail_closed_on_unknown_inherited_workspace_key() {
    // `workspace = true` referencing a key absent from [workspace.dependencies] is Cargo-invalid;
    // the gate must error, not silently treat it as a non-o7-worker dependency.
    let empty = BTreeMap::new();
    let result = analyze(
        PROD,
        r#"
[dependencies]
mystery = { workspace = true, features = ["test-harness"] }
"#,
        &empty,
    );
    assert!(
        result.is_err(),
        "unknown inherited key must fail closed: {result:?}"
    );
}

#[test]
fn adversarial_duplicate_self_dev_alias() {
    // The sanctioned edge plus a SECOND aliased copy of o7-worker in the same table.
    let found = analyze(
        ALLOWED_MANIFEST,
        r#"
[dev-dependencies]
o7-worker = { path = ".", features = ["test-harness"] }
worker-second-copy = { package = "o7-worker", path = ".", features = ["test-harness"] }
"#,
        &BTreeMap::new(),
    )
    .expect("parses");
    let sanctioned = found.iter().filter(|e| e.sanctioned).count();
    let forbidden = found.iter().filter(|e| !e.sanctioned).count();
    assert_eq!(
        sanctioned, 1,
        "exactly the real self-edge is sanctioned: {found:#?}"
    );
    assert_eq!(
        forbidden, 1,
        "the duplicate alias must be flagged: {found:#?}"
    );
}

#[test]
fn adversarial_self_edge_with_wrong_path_is_not_sanctioned() {
    // Same key/table/manifest, but `path` is not "." — must NOT be treated as the sanctioned edge.
    assert_all_forbidden(
        ALLOWED_MANIFEST,
        r#"
[dev-dependencies]
o7-worker = { path = "../o7-worker", features = ["test-harness"] }
"#,
    );
}

#[test]
fn adversarial_production_dependency_edge_is_not_sanctioned() {
    assert_all_forbidden(
        PROD,
        r#"
[dependencies]
o7-worker = { path = "../o7-worker", features = ["test-harness"] }
"#,
    );
}

#[test]
fn fail_closed_on_malformed_manifest() {
    assert!(analyze(PROD, "this is not [ valid toml", &BTreeMap::new()).is_err());
}

#[test]
fn fail_closed_on_unreadable_directory() {
    let missing = Path::new("/definitely/not/a/real/dir/xyzzy");
    assert!(all_manifests(missing).is_err());
}

// ---- binary-lock fixtures ----

fn all_gated_bins() -> String {
    let mut manifest = String::from("\n[package]\nname = \"o7-worker\"\nautobins = false\n");
    for name in HARNESS_BINS {
        manifest.push_str(&format!(
            "\n[[bin]]\nname = \"{name}\"\nrequired-features = [\"test-harness\"]\n"
        ));
    }
    manifest
}

#[test]
fn binary_lock_accepts_the_real_shape() {
    let stems: BTreeSet<String> = HARNESS_BINS.iter().map(|s| (*s).to_owned()).collect();
    binary_policy(&all_gated_bins(), &stems).expect("real shape passes");
}

#[test]
fn binary_lock_rejects_autobins_true() {
    let text = all_gated_bins().replace("autobins = false", "autobins = true");
    let stems: BTreeSet<String> = HARNESS_BINS.iter().map(|s| (*s).to_owned()).collect();
    assert!(binary_policy(&text, &stems).is_err());
}

#[test]
fn binary_lock_rejects_an_ungated_bin() {
    let text = all_gated_bins().replace(
        "[[bin]]\nname = \"fd_probe\"\nrequired-features = [\"test-harness\"]\n",
        "[[bin]]\nname = \"fd_probe\"\n",
    );
    let stems: BTreeSet<String> = HARNESS_BINS.iter().map(|s| (*s).to_owned()).collect();
    assert!(binary_policy(&text, &stems).is_err());
}

#[test]
fn binary_lock_rejects_an_unclassified_source() {
    let mut stems: BTreeSet<String> = HARNESS_BINS.iter().map(|s| (*s).to_owned()).collect();
    stems.insert("rogue".to_owned());
    assert!(binary_policy(&all_gated_bins(), &stems).is_err());
}
