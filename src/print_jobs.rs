// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Crash-conservative print scheduling. No operating-system printer is invoked here.
//!
//! A job MUST be persisted as Pending before any adapter submits it. Pending jobs
//! consume quota until a human reconciles an ambiguous spooler outcome. This
//! prevents over-printing after a crash, at the cost of potentially under-printing.
use crate::{durable_file, print_quota::{PrintAccess, PrintLedger}};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Submitted,
    Confirmed,
    /// Only use after a human has verified the printer never produced the copy.
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PrintJob {
    pub reservation_id: String,
    pub image_path: String,
    pub status: JobStatus,
    #[serde(default)]
    pub spooler_id: Option<String>,
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct PrintJobs {
    #[serde(default)]
    pub jobs: BTreeMap<String, PrintJob>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ScheduleError {
    Quota(PrintAccess),
    DuplicateId,
    InvalidId,
    Storage,
}

impl PrintJobs {
    pub fn open(path: &Path) -> io::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        serde_json::from_slice(&durable_file::read_limited(path, 64 * 1024 * 1024)?).map_err(io::Error::other)
    }

    pub fn outstanding(&self, reservation_id: &str) -> u32 {
        self.jobs.values().filter(|job| {
            job.reservation_id == reservation_id && matches!(job.status, JobStatus::Pending | JobStatus::Submitted)
        }).count().min(u32::MAX as usize) as u32
    }

    pub fn schedule(
        &mut self,
        ledger: &PrintLedger,
        reservation_id: &str,
        job_id: &str,
        image_path: &Path,
        store_path: &Path,
    ) -> Result<(), ScheduleError> {
        if reservation_id.is_empty() || job_id.is_empty() || !image_path.is_file() {
            return Err(ScheduleError::InvalidId);
        }
        if self.jobs.contains_key(job_id) {
            return Err(ScheduleError::DuplicateId);
        }
        // No quota configured for the event = unlimited printing (still journaled).
        // A quota, once set, is a hard limit including jobs not yet confirmed.
        if let Some(quota) = ledger.events.get(reservation_id).filter(|quota| quota.included > 0) {
            let used = quota.completed.saturating_add(self.outstanding(reservation_id));
            if used >= quota.maximum() {
                return Err(ScheduleError::Quota(PrintAccess::LimitReached));
            }
        }
        self.jobs.insert(job_id.to_owned(), PrintJob {
            reservation_id: reservation_id.to_owned(),
            image_path: image_path.to_string_lossy().into_owned(),
            status: JobStatus::Pending,
            spooler_id: None,
        });
        if self.persist(store_path).is_err() {
            self.jobs.remove(job_id);
            return Err(ScheduleError::Storage);
        }
        Ok(())
    }

    /// OS acceptance is not paper completion. Submitted jobs still occupy quota.
    pub fn mark_submitted(&mut self, job_id: &str, spooler_id: String, store_path: &Path) -> io::Result<()> {
        let job = self.jobs.get_mut(job_id).ok_or_else(|| io::Error::other("unknown job"))?;
        if job.status != JobStatus::Pending {
            return Err(io::Error::other("job already handled"));
        }
        job.status = JobStatus::Submitted;
        job.spooler_id = Some(spooler_id);
        if let Err(error) = self.persist(store_path) {
            let job = self.jobs.get_mut(job_id).expect("job exists");
            job.status = JobStatus::Pending;
            job.spooler_id = None;
            return Err(error);
        }
        Ok(())
    }

    /// Confirmation requires an adapter's positive, verified completion signal.
    /// A successful spooler submission alone does not prove printed output.
    pub fn confirm(
        &mut self, ledger: &mut PrintLedger, job_id: &str,
        jobs_path: &Path, ledger_path: &Path,
    ) -> io::Result<()> {
        let job = self.jobs.get(job_id).ok_or_else(|| io::Error::other("unknown job"))?;
        if !matches!(job.status, JobStatus::Pending | JobStatus::Submitted) {
            return Err(io::Error::other("job not pending or submitted"));
        }
        // A crash between ledger persistence and job-state persistence must not
        // increment the completed count a second time during reconciliation.
        if !ledger.confirmed_job_ids.contains(job_id) {
            let quota = ledger.events.get_mut(&job.reservation_id)
                .ok_or_else(|| io::Error::other("reservation missing"))?;
            quota.completed = quota.completed.saturating_add(1);
            ledger.confirmed_job_ids.insert(job_id.to_owned());
            if let Err(error) = ledger.save(ledger_path) {
                ledger.confirmed_job_ids.remove(job_id);
                if let Some(quota) = ledger.events.get_mut(&job.reservation_id) {
                    quota.completed = quota.completed.saturating_sub(1);
                }
                return Err(error);
            }
        }
        self.jobs.get_mut(job_id).expect("checked above").status = JobStatus::Confirmed;
        // Pending may temporarily double-count on failure: this is fail-closed.
        self.persist(jobs_path)
    }

    /// Explicitly checked non-print, reconciled by an operator before releasing quota.
    pub fn cancel_verified_not_printed(&mut self, job_id: &str, store_path: &Path) -> io::Result<()> {
        let job = self.jobs.get_mut(job_id).ok_or_else(|| io::Error::other("unknown job"))?;
        if !matches!(job.status, JobStatus::Pending | JobStatus::Submitted) {
            return Err(io::Error::other("job not pending"));
        }
        let previous = job.status;
        job.status = JobStatus::Cancelled;
        if let Err(error) = self.persist(store_path) {
            self.jobs.get_mut(job_id).expect("job exists").status = previous;
            return Err(error);
        }
        Ok(())
    }

    fn persist(&self, path: &Path) -> io::Result<()> {
        let bytes = serde_json::to_vec(self).map_err(io::Error::other)?;
        durable_file::replace(path, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::print_quota::EventQuota;
    use std::{env, fs, process};

    #[test]
    fn pending_reservations_exhaust_quota_and_survive_restart() {
        let temp = env::temp_dir().join(format!("boothsoft-print-test-{}-{}", process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&temp).unwrap();
        let image = temp.join("photo.jpg");
        fs::write(&image, b"sample").unwrap();
        let jobs_path = temp.join("jobs.json");
        let ledger_path = temp.join("ledger.json");
        let mut ledger = PrintLedger::default();
        ledger.events.insert("booking-123".into(), EventQuota { included: 1, ..Default::default() });
        let mut jobs = PrintJobs::default();
        jobs.schedule(&ledger, "booking-123", "job-1", &image, &jobs_path).unwrap();
        assert_eq!(jobs.schedule(&ledger, "booking-123", "job-2", &image, &jobs_path),
            Err(ScheduleError::Quota(PrintAccess::LimitReached)));
        let mut recovered = PrintJobs::open(&jobs_path).unwrap();
        assert_eq!(recovered.outstanding("booking-123"), 1);
        recovered.confirm(&mut ledger, "job-1", &jobs_path, &ledger_path).unwrap();
        assert_eq!(ledger.events["booking-123"].completed, 1);
        assert_eq!(recovered.outstanding("booking-123"), 0);
        assert_eq!(recovered.schedule(&ledger, "booking-123", "job-2", &image, &jobs_path),
            Err(ScheduleError::Quota(PrintAccess::LimitReached)));
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn confirmation_retry_after_partial_crash_is_idempotent() {
        let temp = env::temp_dir().join(format!("boothsoft-retry-{}-{}", process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        fs::create_dir_all(&temp).unwrap();
        let image = temp.join("photo.jpg");
        fs::write(&image, b"sample").unwrap();
        let jobs_path = temp.join("jobs.json");
        let ledger_path = temp.join("ledger.json");
        let mut ledger = PrintLedger::default();
        ledger.events.insert("event".into(), EventQuota { included: 2, ..Default::default() });
        let mut jobs = PrintJobs::default();
        jobs.schedule(&ledger, "event", "job-1", &image, &jobs_path).unwrap();
        jobs.confirm(&mut ledger, "job-1", &jobs_path, &ledger_path).unwrap();
        assert_eq!(ledger.events["event"].completed, 1);
        // Simulate the on-disk state after saving the ledger but before
        // recording the Confirmed job status.
        jobs.jobs.get_mut("job-1").unwrap().status = JobStatus::Pending;
        jobs.persist(&jobs_path).unwrap();
        let mut recovered_jobs = PrintJobs::open(&jobs_path).unwrap();
        let mut recovered_ledger = PrintLedger::open(&ledger_path).unwrap();
        recovered_jobs.confirm(&mut recovered_ledger, "job-1", &jobs_path, &ledger_path).unwrap();
        assert_eq!(recovered_ledger.events["event"].completed, 1);
        assert_eq!(recovered_jobs.outstanding("event"), 0);
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn submitted_jobs_still_consume_quota_until_confirmed() {
        let temp = env::temp_dir().join(format!("boothsoft-submitted-{}", process::id()));
        fs::create_dir_all(&temp).unwrap();
        let path = temp.join("print.jpg");
        fs::write(&path, b"jpeg").unwrap();
        let jobs_path = temp.join("jobs.json");
        let mut ledger = PrintLedger::default();
        ledger.events.insert("event".into(), EventQuota { included: 1, ..Default::default() });
        let mut jobs = PrintJobs::default();
        jobs.schedule(&ledger, "event", "job-1", &path, &jobs_path).unwrap();
        jobs.mark_submitted("job-1", "spool-123".into(), &jobs_path).unwrap();
        assert_eq!(jobs.outstanding("event"), 1);
        assert_eq!(jobs.schedule(&ledger, "event", "job-2", &path, &jobs_path),
            Err(ScheduleError::Quota(PrintAccess::LimitReached)));
        let recovered = PrintJobs::open(&jobs_path).unwrap();
        assert_eq!(recovered.outstanding("event"), 1);
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn failed_job_transitions_restore_in_memory_state() {
        let mut jobs = PrintJobs::default();
        let ledger = PrintLedger::default();
        let event = "event".to_owned();
        jobs.jobs.insert("job-1".into(), PrintJob {
            reservation_id: event,
            image_path: "photo.jpg".into(),
            status: JobStatus::Pending,
            spooler_id: None,
        });
        let invalid_store = Path::new("nonexistent-boothsoft-directory/store.json");
        assert!(jobs.mark_submitted("job-1", "spool-1".into(), invalid_store).is_err());
        assert_eq!(jobs.jobs["job-1"].status, JobStatus::Pending);
        assert!(jobs.jobs["job-1"].spooler_id.is_none());
        jobs.jobs.get_mut("job-1").unwrap().status = JobStatus::Submitted;
        assert!(jobs.cancel_verified_not_printed("job-1", invalid_store).is_err());
        assert_eq!(jobs.jobs["job-1"].status, JobStatus::Submitted);
        assert_eq!(ledger.events.len(), 0);
    }

    #[test]
    fn no_quota_or_missing_photo_fails_closed() {
        let mut jobs = PrintJobs::default();
        let ledger = PrintLedger::default();
        assert_eq!(jobs.schedule(&ledger, "unknown", "job", Path::new("missing"), Path::new("none")),
            Err(ScheduleError::InvalidId));
    }
}
