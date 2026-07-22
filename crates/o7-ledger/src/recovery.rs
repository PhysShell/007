//! Crash-recovery scanning. On open, the control plane asks the ledger which
//! runs/attempts were still `running` when the previous process stopped. The
//! scan is strictly READ-ONLY: it never changes a status. Marking them
//! `interrupted` is a separate, explicit decision the caller makes via
//! [`SqliteLedger::mark_interrupted`](crate::SqliteLedger::mark_interrupted).

use rusqlite::Connection;

use crate::ids::{AttemptId, RunId};
use crate::models::RecoveryState;
use crate::LedgerError;

/// Collect the runs and attempts currently in `running` state.
///
/// # Errors
/// Propagates SQLite errors.
pub fn scan(conn: &Connection) -> Result<RecoveryState, LedgerError> {
    let mut interrupted_runs = Vec::new();
    {
        let mut stmt = conn.prepare("SELECT run_id FROM run WHERE status = 'running'")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            interrupted_runs.push(RunId::from_raw(row?));
        }
    }

    let mut interrupted_attempts = Vec::new();
    {
        let mut stmt =
            conn.prepare("SELECT attempt_id FROM run_attempt WHERE status = 'running'")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            interrupted_attempts.push(AttemptId::from_raw(row?));
        }
    }

    Ok(RecoveryState {
        interrupted_runs,
        interrupted_attempts,
    })
}
