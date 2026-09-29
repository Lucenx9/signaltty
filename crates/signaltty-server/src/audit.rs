//! JSONL audit log: every broadcast event (except high-volume `pty.data`,
//! which never passes through `Ctx::emit`) appends one line to
//! `state_dir/audit.jsonl`. `subscribe {from_seq}` backfills from the file
//! when the in-memory ring has rotated or the server restarted. Rotation
//! keeps one `.1` predecessor, bounding disk use. Crash honesty matches
//! the snapshot: no fsync per event, the tail may go missing on OS crash.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::store::StoredEvent;

/// Rotation cap: 8 MiB of audit, plus one predecessor.
pub const CAP_BYTES: u64 = 8 * 1024 * 1024;
/// Backfill cap per subscribe: audit + ring merged, deduped by seq.
pub const REPLAY_CAP: usize = 4096;

pub struct AuditLog {
    path: PathBuf,
    cap_bytes: u64,
    file: Mutex<std::io::BufWriter<File>>,
    /// Cached length; refreshed on open and per append.
    len: Mutex<u64>,
}

impl AuditLog {
    pub fn open(state_dir: &Path) -> std::io::Result<AuditLog> {
        Self::open_with_cap(state_dir, CAP_BYTES)
    }

    /// Open or fall back to a disabled log (warns once). The server stays
    /// up with no audit, like snapshot failures which only warn.
    pub fn open_or_disabled(state_dir: &Path) -> AuditLog {
        match AuditLog::open(state_dir) {
            Ok(log) => log,
            Err(e) => {
                tracing::warn!("audit log disabled: {e}");
                AuditLog::disabled()
            }
        }
    }

    /// Append-less fallback when the state dir is unwritable: the server
    /// stays up (like snapshot failures, which only warn) with no audit.
    pub fn disabled() -> AuditLog {
        let file = OpenOptions::new().write(true).open("/dev/null").unwrap();
        AuditLog {
            path: PathBuf::from("/dev/null"),
            cap_bytes: 0,
            file: Mutex::new(std::io::BufWriter::new(file)),
            len: Mutex::new(0),
        }
    }

    fn open_with_cap(state_dir: &Path, cap_bytes: u64) -> std::io::Result<AuditLog> {
        std::fs::create_dir_all(state_dir)?;
        let path = state_dir.join("audit.jsonl");
        let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(AuditLog {
            path,
            cap_bytes,
            file: Mutex::new(std::io::BufWriter::new(file)),
            len: Mutex::new(len),
        })
    }

    fn rotated_path(&self) -> PathBuf {
        self.path.with_extension("jsonl.1")
    }

    /// Append one event line. Best-effort: IO errors are swallowed (the
    /// audit must never break the IPC path) after one loud log.
    pub fn append(&self, seq: u64, name: &str, payload: &Value) {
        if let Err(e) = self.append_fallible(seq, name, payload) {
            tracing::warn!("audit append failed: {e}");
        }
    }

    fn append_fallible(&self, seq: u64, name: &str, payload: &Value) -> std::io::Result<()> {
        self.maybe_rotate()?;
        let line = serde_json::to_string(&json!({
            "seq": seq, "event": name, "payload": payload,
            "at": chrono::Utc::now().to_rfc3339(),
        }))
        .unwrap_or_else(|_| "{\"seq\":0,\"event\":\"encode_failed\"}".to_string());
        let mut file = self.file.lock().unwrap();
        writeln!(file, "{line}")?;
        file.flush()?; // through the OS page cache; no fsync per event (see above)
        *self.len.lock().unwrap() += line.len() as u64 + 1;
        Ok(())
    }

    fn maybe_rotate(&self) -> std::io::Result<()> {
        if self.cap_bytes == 0 || *self.len.lock().unwrap() < self.cap_bytes {
            return Ok(()); // disabled, or under the cap
        }
        let rotated = self.rotated_path();
        let _ = std::fs::remove_file(&rotated);
        // Reopen dance: drop the writer, rename, reopen fresh.
        {
            let mut file = self.file.lock().unwrap();
            file.flush()?;
            *file = std::io::BufWriter::new(
                OpenOptions::new()
                    .create(true)
                    .write(true)
                    .open("/dev/null")?,
            );
        }
        std::fs::rename(&self.path, &rotated)?;
        let fresh = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        *self.file.lock().unwrap() = std::io::BufWriter::new(fresh);
        *self.len.lock().unwrap() = 0;
        Ok(())
    }

    /// Events with `seq > from_seq`, oldest first, capped at `limit`.
    /// Reads the predecessor first so order survives rotation.
    pub fn read_since(&self, from_seq: u64, limit: usize) -> Vec<StoredEvent> {
        let mut out = Vec::new();
        for path in [self.rotated_path(), self.path.clone()] {
            let Ok(file) = File::open(&path) else {
                continue;
            };
            for line in BufReader::new(file).lines().map_while(Result::ok) {
                if out.len() >= limit {
                    break;
                }
                let Ok(v) = serde_json::from_str::<Value>(&line) else {
                    continue; // torn tail line: skip, never fail the replay
                };
                let seq = v.get("seq").and_then(|s| s.as_u64()).unwrap_or(0);
                if seq <= from_seq {
                    continue;
                }
                out.push(StoredEvent {
                    seq,
                    name: v
                        .get("event")
                        .and_then(|e| e.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    payload: v.get("payload").cloned().unwrap_or(Value::Null),
                });
            }
        }
        out.sort_by_key(|e| e.seq);
        out.truncate(limit);
        out
    }
}

/// Merge audit-backfill with the in-memory ring: dedupe by seq, cap total.
/// Audit covers the old range, the ring the recent one; overlap (rotation
/// races, respawn re-emits) collapses on seq.
pub fn merge_replay(backfill: Vec<StoredEvent>, ring: Vec<StoredEvent>) -> Vec<StoredEvent> {
    let mut merged = BTreeMap::new();
    for ev in backfill.into_iter().chain(ring) {
        merged.insert(ev.seq, ev);
    }
    let mut out: Vec<StoredEvent> = merged.into_values().collect();
    out.truncate(REPLAY_CAP);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "signaltty-audit-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn append_read_and_rotation_bound_disk() {
        let d = dir("rotate");
        let log = AuditLog::open_with_cap(&d, 256).unwrap();
        for i in 1..=20u64 {
            log.append(i, "test.event", &json!({"i": i}));
        }
        // Rotation happened: current + one predecessor, bounded total.
        let total: u64 = [log.path.clone(), log.rotated_path()]
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum();
        assert!(total < 256 * 3, "bounded: {total}");
        // Rotation drops old generations by design: survivors replay once,
        // ordered, with the newest seqs intact.
        let events = log.read_since(0, 100);
        let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
        assert!(!seqs.is_empty());
        assert_eq!(seqs, {
            let mut s = seqs.clone();
            s.sort_unstable();
            s.dedup();
            s
        }, "ordered, no dupes");
        assert_eq!(*seqs.last().unwrap(), 20);
        assert!(log.read_since(20, 100).is_empty());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn torn_lines_never_fail_replay() {
        let d = dir("torn");
        let log = AuditLog::open(&d).unwrap();
        log.append(1, "a", &json!({}));
        // Torn tail line between two good appends.
        std::fs::write(log.rotated_path(), "not json\n").ok();
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&log.path)
            .unwrap()
            .write_all(b"{torn\n")
            .unwrap();
        log.append(2, "b", &json!({}));
        let events = log.read_since(0, 100);
        // Torn lines are skipped everywhere; good events replay in order.
        let seqs: Vec<u64> = events.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![1, 2]);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn merge_dedupes_by_seq_and_caps() {
        let ev = |seq| StoredEvent {
            seq,
            name: "e".into(),
            payload: Value::Null,
        };
        let merged = merge_replay(vec![ev(1), ev(2), ev(3)], vec![ev(3), ev(4)]);
        let seqs: Vec<u64> = merged.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![1, 2, 3, 4]);
    }
}
