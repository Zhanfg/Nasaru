// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_policy::Resource;

pub const DEFAULT_MAX_LEASES: usize = 256;
pub const MAX_TTL_MS: u32 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum CapabilityScope {
    Generic = 0,
    Navigation = 1,
    HealthSync = 2,
    ConnectedDevice = 3,
    FileTransfer = 4,
    MediaProcessing = 5,
    VoiceSession = 6,
    CameraSession = 7,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityOrigin {
    TrustedSystemSignal,
    ExplicitUserRule,
    LearnedDecision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapabilityLease {
    pub uid: u32,
    pub resource: Resource,
    pub scope: CapabilityScope,
    pub origin: CapabilityOrigin,
    pub issued_monotonic_ns: u64,
    pub ttl_ms: u32,
}

impl CapabilityLease {
    pub fn is_expired(&self, now_monotonic_ns: u64) -> bool {
        let ttl_ns = u64::from(self.ttl_ms).saturating_mul(1_000_000);
        now_monotonic_ns.saturating_sub(self.issued_monotonic_ns) >= ttl_ns
    }

    fn same_key(&self, other: &Self) -> bool {
        self.uid == other.uid && self.resource == other.resource && self.scope == other.scope
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantOutcome {
    Inserted,
    Replaced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerError {
    ZeroTtl,
    TtlTooLong,
    CapacityReached,
}

#[derive(Debug)]
pub struct CapabilityBroker {
    leases: Vec<CapabilityLease>,
    max_leases: usize,
}

impl Default for CapabilityBroker {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_LEASES)
    }
}

impl CapabilityBroker {
    pub fn new(max_leases: usize) -> Self {
        Self {
            leases: Vec::with_capacity(max_leases.min(DEFAULT_MAX_LEASES)),
            max_leases,
        }
    }

    pub fn grant(
        &mut self,
        lease: CapabilityLease,
        now_monotonic_ns: u64,
    ) -> Result<GrantOutcome, BrokerError> {
        if lease.ttl_ms == 0 {
            return Err(BrokerError::ZeroTtl);
        }
        if lease.ttl_ms > MAX_TTL_MS {
            return Err(BrokerError::TtlTooLong);
        }

        self.expire(now_monotonic_ns);

        if let Some(existing) = self.leases.iter_mut().find(|item| item.same_key(&lease)) {
            *existing = lease;
            return Ok(GrantOutcome::Replaced);
        }

        if self.leases.len() >= self.max_leases {
            return Err(BrokerError::CapacityReached);
        }

        self.leases.push(lease);
        Ok(GrantOutcome::Inserted)
    }

    pub fn has_active(
        &self,
        uid: u32,
        resource: Resource,
        now_monotonic_ns: u64,
    ) -> bool {
        self.leases.iter().any(|lease| {
            lease.uid == uid
                && lease.resource == resource
                && !lease.is_expired(now_monotonic_ns)
        })
    }

    pub fn revoke(&mut self, uid: u32, resource: Resource) -> usize {
        let before = self.leases.len();
        self.leases
            .retain(|lease| !(lease.uid == uid && lease.resource == resource));
        before - self.leases.len()
    }

    pub fn revoke_uid(&mut self, uid: u32) -> usize {
        let before = self.leases.len();
        self.leases.retain(|lease| lease.uid != uid);
        before - self.leases.len()
    }

    pub fn expire(&mut self, now_monotonic_ns: u64) -> usize {
        let before = self.leases.len();
        self.leases
            .retain(|lease| !lease.is_expired(now_monotonic_ns));
        before - self.leases.len()
    }

    pub fn len(&self) -> usize {
        self.leases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leases.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease(uid: u32, resource: Resource, issued: u64, ttl_ms: u32) -> CapabilityLease {
        CapabilityLease {
            uid,
            resource,
            scope: CapabilityScope::Navigation,
            origin: CapabilityOrigin::TrustedSystemSignal,
            issued_monotonic_ns: issued,
            ttl_ms,
        }
    }

    #[test]
    fn lease_expires_without_background_timer() {
        let mut broker = CapabilityBroker::default();
        broker.grant(lease(10001, Resource::Location, 1_000_000_000, 500), 1_000_000_000).unwrap();
        assert!(broker.has_active(10001, Resource::Location, 1_499_999_999));
        assert!(!broker.has_active(10001, Resource::Location, 1_500_000_000));
        assert_eq!(broker.expire(1_500_000_000), 1);
        assert!(broker.is_empty());
    }

    #[test]
    fn same_capability_is_renewed_in_place() {
        let mut broker = CapabilityBroker::default();
        broker.grant(lease(10001, Resource::Location, 100, 100), 100).unwrap();
        let renewed = lease(10001, Resource::Location, 200, 200);
        assert_eq!(broker.grant(renewed, 200).unwrap(), GrantOutcome::Replaced);
        assert_eq!(broker.len(), 1);
        assert!(broker.has_active(10001, Resource::Location, 150_000_000));
    }

    #[test]
    fn revocation_is_per_uid_and_resource() {
        let mut broker = CapabilityBroker::default();
        broker.grant(lease(7, Resource::Location, 0, 1000), 0).unwrap();
        broker.grant(lease(7, Resource::Camera, 0, 1000), 0).unwrap();
        assert_eq!(broker.revoke(7, Resource::Location), 1);
        assert!(!broker.has_active(7, Resource::Location, 1));
        assert!(broker.has_active(7, Resource::Camera, 1));
    }

    #[test]
    fn capacity_is_bounded() {
        let mut broker = CapabilityBroker::new(1);
        broker.grant(lease(1, Resource::Location, 0, 1000), 0).unwrap();
        let result = broker.grant(lease(2, Resource::Location, 0, 1000), 0);
        assert_eq!(result, Err(BrokerError::CapacityReached));
    }
}
