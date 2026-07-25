//! Fail-closed repository gate for o7-worker's `test-harness` feature.
//!
//! STRUCTURAL analysis (a real TOML parse, never a substring grep): every `Cargo.toml` in the
//! workspace tree is parsed, and every edge that ENABLES `test-harness` is located — in any
//! dependency table (`dependencies`, `dev-dependencies`, `build-dependencies`, every
//! `target.*.*dependencies`, and `workspace.dependencies`) or forwarded through any `[features]`
//! array. Exactly one enablement is permitted: o7-worker's own `[dev-dependencies]` self-edge.
//! Any other enablement — a production dependency edge, a build edge, a `foo/test-harness`
//! forward, or a workspace-dependency default — fails the gate. A second test pins the production
//! feature graph: with dev edges excluded, o7-worker resolves without `test-harness`.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::path::{Path, PathBuf};

const FEATURE: &str = "test-harness";
const CRATE: &str = "o7-worker";
/// The ONE sanctioned enablement site (repo-relative manifest, dependency table).
const ALLOWED_MANIFEST: &str = "crates/o7-worker/Cargo.toml";
const ALLOWED_TABLE: &str = "dev-dependencies";

/// Walk up from this crate's manifest to the directory whose `Cargo.toml` declares `[workspace]`.
fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let manifest = dir.join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(&manifest) {
            if let Ok(table) = text.parse::<toml::Table>() {
                if table.contains_key("workspace") {
                    return dir;
                }
            }
        }
        assert!(
            dir.pop(),
            "no [workspace] Cargo.toml found above CARGO_MANIFEST_DIR"
        );
    }
}

/// Every `Cargo.toml` under `root`, skipping `target/` and `.git/`.
fn all_manifests(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if path.is_dir() {
                if name != "target" && name != ".git" {
                    walk(&path, out);
                }
            } else if name == "Cargo.toml" {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

/// Does a dependency `item` (`o7-worker = <item>`) enable `test-harness`?
fn dep_enables_feature(item: &toml::Value) -> bool {
    item.as_table()
        .and_then(|t| t.get("features"))
        .and_then(|f| f.as_array())
        .is_some_and(|arr| arr.iter().any(|v| v.as_str() == Some(FEATURE)))
}

/// Collect `(table_label, table)` for every dependency table in a manifest, including
/// `target.<cfg>.{dependencies,dev-dependencies,build-dependencies}` and `workspace.dependencies`.
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

/// Every enablement of `test-harness` in one manifest, as `(table_label, human_detail)`.
fn enablements(manifest: &Path, root: &Path) -> Vec<(String, String)> {
    let rel = manifest
        .strip_prefix(root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let text = std::fs::read_to_string(manifest).unwrap();
    let table: toml::Table = text.parse().unwrap_or_else(|e| panic!("parse {rel}: {e}"));
    let mut sites = Vec::new();

    // Dependency edges that turn the feature on for the o7-worker dependency.
    for (label, dep_table) in dependency_tables(&table) {
        if let Some(item) = dep_table.get(CRATE) {
            if dep_enables_feature(item) {
                sites.push((
                    label.clone(),
                    format!("{rel} :: [{label}] {CRATE} features = [\"{FEATURE}\"]"),
                ));
            }
        }
    }

    // Feature forwarding: any [features] array entry `o7-worker/test-harness`. (`dep:o7-worker`
    // merely declares an optional dependency and is not a forward, so it is not flagged.)
    let forward = format!("{CRATE}/{FEATURE}");
    if let Some(features) = table.get("features").and_then(toml::Value::as_table) {
        for (fname, arr) in features {
            if let Some(list) = arr.as_array() {
                if list.iter().any(|v| v.as_str() == Some(forward.as_str())) {
                    sites.push((
                        "features".to_owned(),
                        format!("{rel} :: feature \"{fname}\" forwards \"{forward}\""),
                    ));
                }
            }
        }
    }
    sites
}

#[test]
fn test_harness_is_enabled_only_from_the_sanctioned_self_dev_edge() {
    let root = workspace_root();
    let mut violations = Vec::new();
    let mut allowed_seen = false;

    for manifest in all_manifests(&root) {
        let rel = manifest
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        for (label, detail) in enablements(&manifest, &root) {
            let is_sanctioned = rel == ALLOWED_MANIFEST && label == ALLOWED_TABLE;
            if is_sanctioned {
                allowed_seen = true;
            } else {
                violations.push(detail);
            }
        }
    }

    assert!(
        violations.is_empty(),
        "REPOSITORY POLICY VIOLATION — `{FEATURE}` may be enabled only from the o7-worker \
         self dev-dependency ({ALLOWED_MANIFEST} [{ALLOWED_TABLE}]); forbidden enablement(s):\n{}",
        violations.join("\n")
    );
    assert!(
        allowed_seen,
        "the sanctioned self dev-dependency edge ({ALLOWED_MANIFEST} [{ALLOWED_TABLE}] {CRATE} \
         features = [\"{FEATURE}\"]) is missing — the harness lock mechanism was removed or renamed"
    );
}

#[test]
fn production_feature_graph_excludes_the_test_harness_feature() {
    // Production edges only (`no-dev`): the o7-worker node must resolve WITHOUT `test-harness`,
    // proving the feature is reachable solely through the dev self-edge and never in a production
    // build. This is cargo's own structural feature resolution, not a manifest heuristic.
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
        .current_dir(workspace_root())
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
            !(line.contains(CRATE) && line.contains(FEATURE)),
            "production feature graph activates `{FEATURE}` on `{CRATE}`: {line:?}"
        );
    }
}
