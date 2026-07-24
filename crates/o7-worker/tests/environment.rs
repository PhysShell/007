//! Acceptance 6-9: working directory, host-env clearing, explicit env, no shell.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

mod common;

use std::path::PathBuf;

use common::*;

// (6) The explicit working directory is applied.
#[tokio::test]
async fn working_directory_is_applied() {
    let dir = tempfile::tempdir().unwrap();
    let cwd = std::fs::canonicalize(dir.path()).unwrap();
    let sink = RecordingSink::new();
    let result = run_to_completion(child_spec_in("cwd", "print_cwd", cwd.clone()), &sink).await;
    assert_eq!(result.kind(), "EXITED_NORMALLY", "got {result:?}");

    let reported = extract_payload(&sink.stdout()).expect("cwd payload");
    let reported = PathBuf::from(String::from_utf8(reported).unwrap());
    let reported = std::fs::canonicalize(&reported).unwrap_or(reported);
    assert_eq!(reported, cwd);
}

// (7) The host environment is cleared — a var we did not pass (PATH) is absent.
#[tokio::test]
async fn host_environment_is_cleared() {
    let mut spec = child_spec("env-clear", "check_env");
    set_env(&mut spec, ENV_CHECK_VAR, "PATH");
    let sink = RecordingSink::new();
    run_to_completion(spec, &sink).await;
    let reported = extract_payload(&sink.stdout()).expect("env payload");
    assert_eq!(String::from_utf8(reported).unwrap(), "ABSENT");
}

// (8) An explicitly-passed env variable is available to the child.
#[tokio::test]
async fn explicit_env_variable_is_available() {
    let mut spec = child_spec("env-set", "check_env");
    set_env(&mut spec, ENV_CHECK_VAR, "MY_WORKER_VAR");
    set_env(&mut spec, "MY_WORKER_VAR", "hello-123");
    let sink = RecordingSink::new();
    run_to_completion(spec, &sink).await;
    let reported = extract_payload(&sink.stdout()).expect("env payload");
    assert_eq!(String::from_utf8(reported).unwrap(), "PRESENT:hello-123");
}

// (9) No shell interpretation / no PATH search: a relative executable is rejected
// (a shell would have resolved it via PATH).
#[tokio::test]
async fn relative_executable_is_rejected_no_shell() {
    let mut spec = child_spec("rel", "exit0");
    spec.executable = PathBuf::from("true"); // relative → no PATH/shell resolution
    let sink = RecordingSink::new();
    let result = run_to_completion(spec, &sink).await;
    assert_eq!(result.kind(), "FAILED_TO_START", "got {result:?}");
    assert!(!sink.has("spawned"));
}
