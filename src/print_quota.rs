// SPDX-License-Identifier: CPAL-1.0
// Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
//! Business rules for print authorization. This module does not invoke an OS print spooler.
//! Only trusted administrator or device feedback may confirm a physical print.
use crate::durable_file;
use serde::{Deserialize, Serialize};
use std::{collections::{BTreeMap, BTreeSet}, io, path::Path};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PrintLedger {
    #[serde(default)]
    pub events: BTreeMap<String, EventQuota>,
    /// Persisted in the same document as `completed` for crash-idempotent confirms.
    #[serde(default)]
    pub confirmed_job_ids: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EventQuota {
    pub included: u32,
    pub completed: u32,
    pub bonus_unlocked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintAccess {
    #[cfg(test)]
    Allowed,
    LimitReached,
    MissingReservation,
}

impl EventQuota {
    /// At most 5% extra prints; fractions never grant a whole additional print.
    pub fn bonus(&self) -> u32 { self.included / 20 }
    pub fn maximum(&self) -> u32 {
        self.included.saturating_add(if self.bonus_unlocked { self.bonus() } else { 0 })
    }
    #[cfg(test)]
    pub fn check(&self) -> PrintAccess {
        if self.included == 0 { PrintAccess::MissingReservation }
        else if self.completed >= self.maximum() { PrintAccess::LimitReached }
        else { PrintAccess::Allowed }
    }
    #[cfg(test)]
    pub fn record_success(&mut self) -> Result<(), PrintAccess> {
        match self.check() {
            PrintAccess::Allowed => { self.completed += 1; Ok(()) }
            error => Err(error),
        }
    }
}

impl PrintLedger {
    /// Commit quota updates only when they cannot undercut already confirmed
    /// or outstanding print jobs. Do not mutate in-memory state before disk
    /// persistence succeeds.
    pub fn update_included(
        &mut self, event_id: &str, requested: u32, outstanding: u32, path: &Path,
    ) -> io::Result<()> {
        if event_id.is_empty() || requested > 100_000 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid event or quota"));
        }
        let original = self.events.get(event_id).cloned();
        let quota = original.clone().unwrap_or_default();
        let reserved = quota.completed.saturating_add(outstanding);
        if requested < reserved {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                "Quota cannot be lower than confirmed and outstanding prints"));
        }
        self.events.entry(event_id.to_owned()).or_default().included = requested;
        if let Err(error) = self.save(path) {
            match original {
                Some(previous) => { self.events.insert(event_id.to_owned(), previous); }
                None => { self.events.remove(event_id); }
            }
            return Err(error);
        }
        Ok(())
    }

    /// Bonus unlock is an administrator-only, durable state transition.
    pub fn unlock_bonus(&mut self, event_id: &str, path: &Path) -> io::Result<()> {
        let quota = self.events.get_mut(event_id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No event quota"))?;
        if quota.included == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "No event quota"));
        }
        if quota.bonus_unlocked { return Ok(()); }
        quota.bonus_unlocked = true;
        if let Err(error) = self.save(path) {
            self.events.get_mut(event_id).expect("validated event").bonus_unlocked = false;
            return Err(error);
        }
        Ok(())
    }

    pub fn open(path: &Path) -> io::Result<Self> {
        if !path.exists() { return Ok(Self::default()); }
        serde_json::from_slice(&durable_file::read_limited(path, 32 * 1024 * 1024)?).map_err(io::Error::other)
    }
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let content = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        durable_file::replace(path, &content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quota_change_fails_closed_when_existing_prints_exceed_requested() {
        let mut ledger = PrintLedger::default();
        ledger.events.insert("wedding".into(), EventQuota {
            included: 10, completed: 6, bonus_unlocked: false,
        });
        let path = std::env::temp_dir().join(format!("boothsoft-ledger-{}-{}.json",
            std::process::id(), std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        assert!(ledger.update_included("wedding", 7, 2, &path).is_err());
        assert_eq!(ledger.events["wedding"].included, 10);
        ledger.update_included("wedding", 8, 2, &path).unwrap();
        assert_eq!(ledger.events["wedding"].included, 8);
        ledger.unlock_bonus("wedding", &path).unwrap();
        assert!(PrintLedger::open(&path).unwrap().events["wedding"].bonus_unlocked);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn failed_storage_write_leaves_ledger_unchanged() {
        let mut ledger = PrintLedger::default();
        let nonexistent = std::env::temp_dir()
            .join(format!("missing-boothsoft-{}", std::process::id()))
            .join("ledger.json");
        assert!(ledger.update_included("event", 5, 0, &nonexistent).is_err());
        assert!(ledger.events.is_empty());
    }
    #[test]
    fn quota_stops_at_contract_and_bonus_requires_unlock() {
        let mut q = EventQuota { included: 100, completed: 100, bonus_unlocked: false };
        assert_eq!(q.check(), PrintAccess::LimitReached);
        q.bonus_unlocked = true;
        assert_eq!(q.maximum(), 105);
        for _ in 0..5 { assert!(q.record_success().is_ok()); }
        assert_eq!(q.record_success(), Err(PrintAccess::LimitReached));
    }
    #[test]
    fn old_ledger_without_confirmed_ids_is_compatible() {
        let ledger: PrintLedger = serde_json::from_str(r#"{"events":{}}"#).unwrap();
        assert!(ledger.confirmed_job_ids.is_empty());
    }
    #[test]
    fn no_reservation_fails_closed() {
        assert_eq!(EventQuota::default().check(), PrintAccess::MissingReservation);
    }
    #[test]
    fn bonus_rounds_up() {
        let q = EventQuota { included: 21, ..Default::default() };
        assert_eq!(q.bonus(), 1);
        assert_eq!(EventQuota { included: 19, ..Default::default() }.bonus(), 0);
    }
}
