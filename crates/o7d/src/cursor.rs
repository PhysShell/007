//! Wire encoding for [`o7_ledger::ListCursor`]. The ledger's cursor is a plain
//! `(created_at, tiebreak)` pair; this module is the ONLY place that turns it
//! into (and back out of) an opaque `before=` query value, so the encoding
//! rule exists exactly once.

use o7_ledger::ListCursor;

/// `created_at.tiebreak` — `.` never appears in either field's decimal `i64`
/// rendering, so a single `split_once` is unambiguous.
pub fn encode(cursor: &ListCursor) -> String {
    format!("{}.{}", cursor.created_at, cursor.tiebreak)
}

/// Rejects anything that is not exactly two dot-separated integers — a
/// malformed cursor is a 400, never silently clamped or ignored.
pub fn decode(raw: &str) -> Result<ListCursor, String> {
    let (a, b) = raw
        .split_once('.')
        .ok_or_else(|| "cursor must be `created_at.tiebreak`".to_owned())?;
    let created_at = a
        .parse::<i64>()
        .map_err(|_| "cursor's created_at is not an integer".to_owned())?;
    let tiebreak = b
        .parse::<i64>()
        .map_err(|_| "cursor's tiebreak is not an integer".to_owned())?;
    Ok(ListCursor {
        created_at,
        tiebreak,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let c = ListCursor {
            created_at: 1_753_000_000_123,
            tiebreak: 42,
        };
        assert_eq!(decode(&encode(&c)).unwrap(), c);
    }

    #[test]
    fn round_trips_negative_created_at() {
        // created_at is a raw millis i64; not expected negative in practice,
        // but the codec must not silently misparse one rather than reject it.
        let c = ListCursor {
            created_at: -5,
            tiebreak: 7,
        };
        assert_eq!(decode(&encode(&c)).unwrap(), c);
    }

    #[test]
    fn rejects_missing_dot() {
        assert!(decode("12345").is_err());
    }

    #[test]
    fn rejects_non_numeric() {
        assert!(decode("abc.def").is_err());
        assert!(decode("12.def").is_err());
    }

    #[test]
    fn rejects_empty() {
        assert!(decode("").is_err());
        assert!(decode(".").is_err());
    }
}
