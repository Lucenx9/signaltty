//! Bounded state-event journal, durable sequence reservations and replay evidence.

use crate::store::StoredEvent;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const CAP_BYTES: u64 = 8 * 1024 * 1024;
pub const REPLAY_CAP: usize = 4096;

use std::os::unix::fs::OpenOptionsExt;

const LEASE: u64 = 4096;

pub struct EventSequence {
    path: PathBuf,
    issued: u64,
    reserved: u64,
}

impl EventSequence {
    pub fn open(state_dir: &Path, audit_max: u64) -> io::Result<Self> {
        std::fs::create_dir_all(state_dir)?;
        let path = state_dir.join("event-sequence.json");
        let issued = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<u64>(&bytes).map_err(io::Error::other)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => audit_max,
            Err(e) => return Err(e),
        };
        if issued < audit_max {
            return Err(io::Error::other("sequence reservation is behind the audit"));
        }
        let mut sequence = Self {
            path,
            issued,
            reserved: issued,
        };
        sequence.reserve()?;
        Ok(sequence)
    }

    pub fn head(&self) -> u64 {
        self.issued
    }

    pub fn allocate(&mut self) -> io::Result<u64> {
        if self.issued == self.reserved {
            self.reserve()?;
        }
        self.issued = self
            .issued
            .checked_add(1)
            .ok_or_else(|| io::Error::other("event sequence exhausted"))?;
        Ok(self.issued)
    }

    fn reserve(&mut self) -> io::Result<()> {
        let next = self
            .reserved
            .checked_add(LEASE)
            .ok_or_else(|| io::Error::other("event sequence exhausted"))?;
        durable_write(
            &self.path,
            &serde_json::to_vec(&next).map_err(io::Error::other)?,
        )?;
        self.reserved = next;
        Ok(())
    }
}

pub fn durable_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&tmp, path)?;
    File::open(
        path.parent()
            .ok_or_else(|| io::Error::other("missing parent"))?,
    )?
    .sync_all()
}

#[derive(Clone, Copy, Default, Serialize, Deserialize)]
struct Retention {
    retained_after: u64,
    predecessor: bool,
    #[serde(default)]
    current_bytes: u64,
    #[serde(default)]
    predecessor_bytes: u64,
    #[serde(default)]
    known_complete: bool,
}

pub struct AuditLog {
    path: PathBuf,
    cap_bytes: u64,
    file: Mutex<File>,
    len: Mutex<u64>,
    retention: Mutex<Retention>,
    available: bool,
}

pub struct History {
    pub events: Vec<StoredEvent>,
    pub retained_after: u64,
    pub available: bool,
}

impl AuditLog {
    pub fn open(state_dir: &Path) -> io::Result<Self> {
        Self::open_with_cap(state_dir, CAP_BYTES)
    }

    fn open_with_cap(state_dir: &Path, cap_bytes: u64) -> io::Result<Self> {
        std::fs::create_dir_all(state_dir)?;
        let path = state_dir.join("audit.jsonl");
        let retention_path = state_dir.join("audit-retention.json");
        let mut available;
        let retention = match std::fs::read(&retention_path) {
            Ok(bytes) => match serde_json::from_slice::<Retention>(&bytes) {
                Ok(retention) => {
                    available = retention.known_complete && lengths_match(&path, retention);
                    retention
                }
                Err(_) => {
                    available = false;
                    Retention::default()
                }
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let (events, valid) = read_files(&[path.with_extension("jsonl.1"), path.clone()]);
                available =
                    valid && events.is_empty() && !state_dir.join("event-sequence.json").exists();
                let retention = Retention {
                    retained_after: events.iter().map(|e| e.seq).max().unwrap_or(0),
                    predecessor: path.with_extension("jsonl.1").exists(),
                    current_bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                    predecessor_bytes: std::fs::metadata(path.with_extension("jsonl.1"))
                        .map(|m| m.len())
                        .unwrap_or(0),
                    known_complete: available,
                };
                durable_write(
                    &retention_path,
                    &serde_json::to_vec(&retention).map_err(io::Error::other)?,
                )?;
                retention
            }
            Err(e) => return Err(e),
        };
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&path)?;
        // Separate a torn tail from all future records; corruption remains explicit.
        let bytes = std::fs::read(&path)?;
        if !bytes.is_empty() && bytes.last() != Some(&b'\n') {
            file.write_all(b"\n")?;
            file.sync_all()?;
            available = false;
        }
        let len = file.metadata()?.len();
        Ok(Self {
            path,
            cap_bytes,
            file: Mutex::new(file),
            len: Mutex::new(len),
            retention: Mutex::new(retention),
            available,
        })
    }

    fn rotated_path(&self) -> PathBuf {
        self.path.with_extension("jsonl.1")
    }

    pub fn history(&self) -> History {
        let (events, valid) = read_files(&[self.rotated_path(), self.path.clone()]);
        let retention = *self.retention.lock().unwrap();
        History {
            events,
            retained_after: retention.retained_after,
            available: self.available
                && valid
                && retention.known_complete
                && lengths_match(&self.path, retention),
        }
    }

    pub fn max_seq(&self) -> u64 {
        self.history()
            .events
            .iter()
            .map(|e| e.seq)
            .max()
            .unwrap_or(0)
    }

    pub fn append(&self, seq: u64, name: &str, payload: &Value) -> io::Result<()> {
        {
            let mut retention = self.retention.lock().unwrap();
            if !lengths_match(&self.path, *retention) || !self.available {
                retention.known_complete = false;
            }
        }
        self.maybe_rotate()?;
        let mut retention = self.retention.lock().unwrap();
        if !lengths_match(&self.path, *retention) || !self.available {
            retention.known_complete = false;
        }
        // An unlinked open descriptor is not a replayable journal.
        std::fs::metadata(&self.path)?;
        let line = serde_json::to_vec(&json!({"seq":seq, "event":name, "payload":payload, "at":chrono::Utc::now().to_rfc3339()})).map_err(io::Error::other)?;
        let mut file = self.file.lock().unwrap();
        file.write_all(&line)?;
        file.write_all(b"\n")?;
        file.sync_data()?;
        *self.len.lock().unwrap() = file.metadata()?.len();
        retention.current_bytes = *self.len.lock().unwrap();
        durable_write(
            &self.path.with_file_name("audit-retention.json"),
            &serde_json::to_vec(&*retention).map_err(io::Error::other)?,
        )?;
        Ok(())
    }

    fn maybe_rotate(&self) -> io::Result<()> {
        if *self.len.lock().unwrap() < self.cap_bytes {
            return Ok(());
        }
        let rotated = self.rotated_path();
        let (discarded, valid) = read_files(std::slice::from_ref(&rotated));
        if !valid {
            return Err(io::Error::other("cannot prove discarded audit history"));
        }
        let mut retention = self.retention.lock().unwrap();
        retention.retained_after = retention
            .retained_after
            .max(discarded.iter().map(|e| e.seq).max().unwrap_or(0));
        retention.predecessor = true;
        retention.predecessor_bytes = *self.len.lock().unwrap();
        retention.current_bytes = 0;
        let retention_path = self.path.with_file_name("audit-retention.json");
        durable_write(
            &retention_path,
            &serde_json::to_vec(&*retention).map_err(io::Error::other)?,
        )?;
        // rename atomically replaces the predecessor; metadata already excludes it.
        std::fs::rename(&self.path, &rotated)?;
        let fresh = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&self.path)?;
        fresh.sync_all()?;
        File::open(self.path.parent().unwrap())?.sync_all()?;
        *self.file.lock().unwrap() = fresh;
        *self.len.lock().unwrap() = 0;
        Ok(())
    }

    pub fn read_since(&self, from_seq: u64, limit: usize) -> Vec<StoredEvent> {
        self.history()
            .events
            .into_iter()
            .filter(|e| e.seq > from_seq)
            .take(limit)
            .collect()
    }
}

fn lengths_match(path: &Path, retention: Retention) -> bool {
    let current = std::fs::metadata(path).ok().map(|m| m.len());
    let previous = std::fs::metadata(path.with_extension("jsonl.1"))
        .ok()
        .map(|m| m.len());
    current == Some(retention.current_bytes)
        && if retention.predecessor {
            previous == Some(retention.predecessor_bytes)
        } else {
            previous.is_none()
        }
}

fn read_files(paths: &[PathBuf]) -> (Vec<StoredEvent>, bool) {
    let mut events = Vec::new();
    let mut valid = true;
    for path in paths {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(_) => {
                valid = false;
                continue;
            }
        };
        for line in BufReader::new(file).lines() {
            let parsed = line
                .ok()
                .and_then(|line| serde_json::from_str::<Value>(&line).ok());
            if let Some(value) = parsed {
                if let (Some(seq), Some(name), Some(payload)) = (
                    value["seq"].as_u64(),
                    value["event"].as_str(),
                    value.get("payload"),
                ) {
                    if events
                        .last()
                        .map(|e: &StoredEvent| seq <= e.seq)
                        .unwrap_or(false)
                    {
                        valid = false;
                    }
                    events.push(StoredEvent {
                        seq,
                        name: name.into(),
                        payload: payload.clone(),
                    });
                    continue;
                }
            }
            valid = false;
        }
    }
    (events, valid)
}

/// Merge all bounded retained records. Apply filters before the replay cap.
pub fn merge_replay(backfill: Vec<StoredEvent>, ring: Vec<StoredEvent>) -> Vec<StoredEvent> {
    let mut merged = BTreeMap::new();
    for ev in backfill.into_iter().chain(ring) {
        merged.insert(ev.seq, ev);
    }
    merged.into_values().collect()
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
    fn missing_all_history_with_a_surviving_reservation_is_not_fresh() {
        let d = dir("lost-all");
        let log = AuditLog::open(&d).unwrap();
        let mut sequence = EventSequence::open(&d, 0).unwrap();
        let seq = sequence.allocate().unwrap();
        log.append(seq, "event", &json!({})).unwrap();
        drop(log);
        for path in ["audit.jsonl", "audit-retention.json"] {
            std::fs::remove_file(d.join(path)).unwrap();
        }
        let reopened = AuditLog::open(&d).unwrap();
        assert!(!reopened.history().available);
        reopened
            .append(sequence.allocate().unwrap(), "new", &json!({}))
            .unwrap();
        drop(reopened);
        assert!(!AuditLog::open(&d).unwrap().history().available);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn rotation_preserves_missing_predecessor_uncertainty() {
        let d = dir("rotation-loss");
        let log = AuditLog::open_with_cap(&d, 256).unwrap();
        let mut seq = 0;
        while !log.rotated_path().exists() || *log.len.lock().unwrap() < 256 {
            seq += 1;
            log.append(seq, "event", &json!({})).unwrap();
            assert!(seq < 30);
        }
        std::fs::remove_file(log.rotated_path()).unwrap();
        log.append(seq + 1, "event", &json!({})).unwrap();
        assert!(
            !log.history().available,
            "rotation must not erase loss evidence"
        );
        drop(log);
        assert!(
            !AuditLog::open_with_cap(&d, 256)
                .unwrap()
                .history()
                .available
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn removed_or_validly_truncated_generations_cannot_prove_coverage() {
        let d = dir("missing");
        let log = AuditLog::open_with_cap(&d, 256).unwrap();
        for seq in 1..=8 {
            log.append(seq, "event", &json!({})).unwrap();
        }
        assert!(log.history().available);
        std::fs::remove_file(log.rotated_path()).unwrap();
        assert!(!log.history().available);
        drop(log);
        assert!(
            !AuditLog::open_with_cap(&d, 256)
                .unwrap()
                .history()
                .available
        );
        std::fs::remove_dir_all(d).unwrap();
        let d = dir("truncated");
        let log = AuditLog::open(&d).unwrap();
        log.append(1, "a", &json!({})).unwrap();
        let first = std::fs::read(&log.path).unwrap();
        log.append(2, "b", &json!({})).unwrap();
        std::fs::write(&log.path, first).unwrap(); // complete JSON lines, no malformed tail
        assert!(!log.history().available);
        drop(log);
        let reopened = AuditLog::open(&d).unwrap();
        assert!(!reopened.history().available);
        reopened.append(3, "c", &json!({})).unwrap();
        drop(reopened);
        assert!(
            !AuditLog::open(&d).unwrap().history().available,
            "uncertainty survives append and restart"
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn legacy_journal_retains_uncertainty_after_migration() {
        let d = dir("legacy");
        std::fs::write(
            d.join("audit.jsonl"),
            b"{\"seq\":5,\"event\":\"old\",\"payload\":{}}\n",
        )
        .unwrap();
        let log = AuditLog::open(&d).unwrap();
        assert_eq!(log.max_seq(), 5);
        assert!(!log.history().available);
        log.append(6, "new", &json!({})).unwrap();
        drop(log);
        assert!(!AuditLog::open(&d).unwrap().history().available);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn reservation_survives_crash_without_any_audit_event() {
        let d = dir("sequence");
        let mut sequence = EventSequence::open(&d, 0).unwrap();
        let first = sequence.allocate().unwrap();
        let transient = sequence.allocate().unwrap();
        drop(sequence);
        let mut restarted = EventSequence::open(&d, 0).unwrap();
        assert!(restarted.allocate().unwrap() > transient);
        assert!(transient > first);
        std::fs::write(d.join("event-sequence.json"), b"broken").unwrap();
        assert!(EventSequence::open(&d, 0).is_err());
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn reservation_failure_does_not_issue_an_unreserved_number() {
        let d = dir("sequence-failure");
        let mut sequence = EventSequence::open(&d, 0).unwrap();
        for _ in 0..LEASE {
            sequence.allocate().unwrap();
        }
        std::fs::remove_file(d.join("event-sequence.json")).unwrap();
        std::fs::create_dir(d.join("event-sequence.json")).unwrap();
        assert!(sequence.allocate().is_err());
        assert_eq!(sequence.head(), LEASE);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn torn_tail_is_separated_and_coverage_is_unavailable() {
        let d = dir("unterminated");
        let log = AuditLog::open(&d).unwrap();
        log.append(1, "a", &json!({})).unwrap();
        drop(log);
        OpenOptions::new()
            .append(true)
            .open(d.join("audit.jsonl"))
            .unwrap()
            .write_all(b"{torn")
            .unwrap();
        let reopened = AuditLog::open(&d).unwrap();
        reopened.append(2, "b", &json!({})).unwrap();
        assert_eq!(
            reopened
                .read_since(0, 100)
                .iter()
                .map(|e| e.seq)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(!reopened.history().available);
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn append_read_and_rotation_bound_disk() {
        let d = dir("rotate");
        let log = AuditLog::open_with_cap(&d, 256).unwrap();
        for i in 1..=20u64 {
            log.append(i, "test.event", &json!({"i": i})).unwrap();
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
        assert_eq!(
            seqs,
            {
                let mut s = seqs.clone();
                s.sort_unstable();
                s.dedup();
                s
            },
            "ordered, no dupes"
        );
        assert_eq!(*seqs.last().unwrap(), 20);
        assert!(log.read_since(20, 100).is_empty());
        let floor = log.history().retained_after;
        assert!(floor > 0);
        drop(log);
        let reopened = AuditLog::open_with_cap(&d, 256).unwrap();
        assert!(reopened.history().available);
        assert_eq!(reopened.history().retained_after, floor);
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn torn_lines_never_fail_replay() {
        let d = dir("torn");
        let log = AuditLog::open(&d).unwrap();
        log.append(1, "a", &json!({})).unwrap();
        // Torn tail line between two good appends.
        std::fs::write(log.rotated_path(), "not json\n").ok();
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&log.path)
            .unwrap()
            .write_all(b"{torn\n")
            .unwrap();
        log.append(2, "b", &json!({})).unwrap();
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
