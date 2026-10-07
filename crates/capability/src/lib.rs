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
    pub fn expiry_monotonic_ns(&self) -> u64 {
        self.issued_monotonic_ns
            .saturating_add(u64::from(self.ttl_ms).saturating_mul(1_000_000))
    }

    pub fn is_expired(&self, now_monotonic_ns: u64) -> bool {
        now_monotonic_ns >= self.expiry_monotonic_ns()
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

    pub fn has_active(&self, uid: u32, resource: Resource, now_monotonic_ns: u64) -> bool {
        self.leases.iter().any(|lease| {
            lease.uid == uid && lease.resource == resource && !lease.is_expired(now_monotonic_ns)
        })
    }

    pub fn revoke(&mut self, uid: u32, resource: Resource) -> usize {
        self.revoke_collect(uid, resource).len()
    }

    pub fn revoke_collect(&mut self, uid: u32, resource: Resource) -> Vec<CapabilityLease> {
        self.remove_matching(|lease| lease.uid == uid && lease.resource == resource)
    }

    pub fn revoke_scoped(&mut self, uid: u32, resource: Resource, scope: CapabilityScope) -> usize {
        self.remove_matching(|lease| {
            lease.uid == uid && lease.resource == resource && lease.scope == scope
        })
        .len()
    }

    pub fn revoke_uid(&mut self, uid: u32) -> usize {
        self.remove_matching(|lease| lease.uid == uid).len()
    }

    pub fn expire(&mut self, now_monotonic_ns: u64) -> usize {
        self.expire_collect(now_monotonic_ns).len()
    }

    pub fn expire_collect(&mut self, now_monotonic_ns: u64) -> Vec<CapabilityLease> {
        self.remove_matching(|lease| lease.is_expired(now_monotonic_ns))
    }

    pub fn next_expiry_ns(&self) -> Option<u64> {
        self.leases
            .iter()
            .map(CapabilityLease::expiry_monotonic_ns)
            .min()
    }

    pub fn len(&self) -> usize {
        self.leases.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leases.is_empty()
    }

    fn remove_matching<F>(&mut self, mut predicate: F) -> Vec<CapabilityLease>
    where
        F: FnMut(&CapabilityLease) -> bool,
    {
        let old = std::mem::take(&mut self.leases);
        let mut removed = Vec::new();
        self.leases.reserve(old.len());

        for lease in old {
            if predicate(&lease) {
                removed.push(lease);
            } else {
                self.leases.push(lease);
            }
        }
        removed
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
    fn lease_expires_without_polling() {
        let mut broker = CapabilityBroker::default();
        broker
            .grant(
                lease(10001, Resource::Location, 1_000_000_000, 500),
                1_000_000_000,
            )
            .unwrap();
        assert_eq!(broker.next_expiry_ns(), Some(1_500_000_000));
        assert!(broker.has_active(10001, Resource::Location, 1_499_999_999));
        let expired = broker.expire_collect(1_500_000_000);
        assert_eq!(expired.len(), 1);
        assert!(broker.is_empty());
    }

    #[test]
    fn same_capability_is_renewed_in_place() {
        let mut broker = CapabilityBroker::default();
        broker
            .grant(lease(10001, Resource::Location, 100, 100), 100)
            .unwrap();
        let renewed = lease(10001, Resource::Location, 200, 200);
        assert_eq!(broker.grant(renewed, 200).unwrap(), GrantOutcome::Replaced);
        assert_eq!(broker.len(), 1);
        assert!(broker.has_active(10001, Resource::Location, 150_000_000));
    }

    #[test]
    fn revocation_returns_exact_removed_leases() {
        let mut broker = CapabilityBroker::default();
        broker
            .grant(lease(7, Resource::Location, 0, 1000), 0)
            .unwrap();
        broker
            .grant(lease(7, Resource::Camera, 0, 1000), 0)
            .unwrap();
        let removed = broker.revoke_collect(7, Resource::Location);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].resource, Resource::Location);
        assert!(broker.has_active(7, Resource::Camera, 1));
    }

    #[test]
    fn scoped_revocation_preserves_other_session_on_same_resource() {
        let mut broker = CapabilityBroker::default();
        broker
            .grant(
                CapabilityLease {
                    uid: 77,
                    resource: Resource::NetworkEgress,
                    scope: CapabilityScope::Navigation,
                    origin: CapabilityOrigin::TrustedSystemSignal,
                    issued_monotonic_ns: 0,
                    ttl_ms: 10_000,
                },
                0,
            )
            .unwrap();
        broker
            .grant(
                CapabilityLease {
                    uid: 77,
                    resource: Resource::NetworkEgress,
                    scope: CapabilityScope::VoiceSession,
                    origin: CapabilityOrigin::TrustedSystemSignal,
                    issued_monotonic_ns: 0,
                    ttl_ms: 10_000,
                },
                0,
            )
            .unwrap();

        assert_eq!(
            broker.revoke_scoped(77, Resource::NetworkEgress, CapabilityScope::Navigation),
            1
        );
        assert!(broker.has_active(77, Resource::NetworkEgress, 1));
        assert_eq!(broker.len(), 1);
    }

    #[test]
    fn capacity_is_bounded() {
        let mut broker = CapabilityBroker::new(1);
        broker
            .grant(lease(1, Resource::Location, 0, 1000), 0)
            .unwrap();
        let result = broker.grant(lease(2, Resource::Location, 0, 1000), 0);
        assert_eq!(result, Err(BrokerError::CapacityReached));
    }
}
