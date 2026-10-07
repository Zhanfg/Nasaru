// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_capability::{
    BrokerError, CapabilityBroker, CapabilityLease, CapabilityOrigin, CapabilityScope,
};
use nasaru_policy::{decide, Context, Decision, Reason, Resource};

#[derive(Debug, Clone, Copy, Default)]
pub struct TrustedContext {
    pub trusted_system_context: bool,
    pub foreground: bool,
    pub user_initiated: bool,
    pub existing_handle_or_flow: bool,
    pub recent_blocked_sensitive_read: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct AccessRequest {
    pub uid: u32,
    pub resource: Resource,
    pub monotonic_ns: u64,
    pub context: TrustedContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelAdvice {
    AllowTemporary { scope: CapabilityScope, ttl_ms: u32 },
    Deny,
    Abstain,
}

pub trait ModelAdvisor {
    fn advise(&mut self, request: &AccessRequest) -> ModelAdvice;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardDecision {
    Allow(Reason),
    AllowTemporary { scope: CapabilityScope, ttl_ms: u32 },
    Deny(Reason),
    DenyByModel,
    DeferToPlatform,
    UnsupportedResource,
    BrokerFailure(BrokerError),
}

#[derive(Debug)]
pub struct ApplicationGuard<A> {
    broker: CapabilityBroker,
    advisor: A,
}

impl<A: ModelAdvisor> ApplicationGuard<A> {
    pub fn new(broker: CapabilityBroker, advisor: A) -> Self {
        Self { broker, advisor }
    }

    pub fn broker(&self) -> &CapabilityBroker {
        &self.broker
    }

    pub fn broker_mut(&mut self) -> &mut CapabilityBroker {
        &mut self.broker
    }

    pub fn evaluate(&mut self, request: AccessRequest) -> GuardDecision {
        if request.resource == Resource::NetworkEgress {
            return GuardDecision::UnsupportedResource;
        }

        let active_capability =
            self.broker
                .has_active(request.uid, request.resource, request.monotonic_ns);

        let policy_context = Context {
            trusted_system_context: request.context.trusted_system_context,
            foreground: request.context.foreground,
            user_initiated: request.context.user_initiated,
            existing_handle_or_flow: request.context.existing_handle_or_flow,
            active_capability,
            recent_blocked_sensitive_read: request.context.recent_blocked_sensitive_read,
        };

        match decide(request.resource, policy_context) {
            Decision::Allow(reason) => GuardDecision::Allow(reason),
            Decision::Deny(reason) => GuardDecision::Deny(reason),
            Decision::NeedModel(_) => self.evaluate_ambiguous(request),
        }
    }

    fn evaluate_ambiguous(&mut self, request: AccessRequest) -> GuardDecision {
        match self.advisor.advise(&request) {
            ModelAdvice::AllowTemporary { scope, ttl_ms } => {
                let lease = CapabilityLease {
                    uid: request.uid,
                    resource: request.resource,
                    scope,
                    origin: CapabilityOrigin::LearnedDecision,
                    issued_monotonic_ns: request.monotonic_ns,
                    ttl_ms,
                };
                match self.broker.grant(lease, request.monotonic_ns) {
                    Ok(_) => GuardDecision::AllowTemporary { scope, ttl_ms },
                    Err(error) => GuardDecision::BrokerFailure(error),
                }
            }
            ModelAdvice::Deny => GuardDecision::DenyByModel,
            ModelAdvice::Abstain => GuardDecision::DeferToPlatform,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct RecordingAdvisor {
        calls: usize,
        advice: Option<ModelAdvice>,
    }

    impl ModelAdvisor for RecordingAdvisor {
        fn advise(&mut self, _request: &AccessRequest) -> ModelAdvice {
            self.calls += 1;
            self.advice.unwrap_or(ModelAdvice::Abstain)
        }
    }

    fn request(resource: Resource) -> AccessRequest {
        AccessRequest {
            uid: 10042,
            resource,
            monotonic_ns: 1_000_000_000,
            context: TrustedContext::default(),
        }
    }

    #[test]
    fn model_cannot_override_hard_camera_invariant() {
        let advisor = RecordingAdvisor {
            advice: Some(ModelAdvice::AllowTemporary {
                scope: CapabilityScope::CameraSession,
                ttl_ms: 30_000,
            }),
            ..RecordingAdvisor::default()
        };
        let mut guard = ApplicationGuard::new(CapabilityBroker::default(), advisor);

        assert_eq!(
            guard.evaluate(request(Resource::Camera)),
            GuardDecision::Deny(Reason::HardBackgroundSensorInvariant)
        );
        assert_eq!(guard.advisor.calls, 0);
    }

    #[test]
    fn broker_not_caller_controls_active_capability() {
        let mut guard =
            ApplicationGuard::new(CapabilityBroker::default(), RecordingAdvisor::default());
        let mut req = request(Resource::Location);
        req.context.user_initiated = false;

        assert_eq!(
            guard.evaluate(req),
            GuardDecision::Deny(Reason::HardBackgroundSensorInvariant)
        );
    }

    #[test]
    fn trusted_navigation_lease_allows_background_location() {
        let mut broker = CapabilityBroker::default();
        broker
            .grant(
                CapabilityLease {
                    uid: 10042,
                    resource: Resource::Location,
                    scope: CapabilityScope::Navigation,
                    origin: CapabilityOrigin::TrustedSystemSignal,
                    issued_monotonic_ns: 1_000_000_000,
                    ttl_ms: 300_000,
                },
                1_000_000_000,
            )
            .unwrap();

        let mut guard = ApplicationGuard::new(broker, RecordingAdvisor::default());
        assert_eq!(
            guard.evaluate(request(Resource::Location)),
            GuardDecision::Allow(Reason::ActiveCapability)
        );
    }

    #[test]
    fn expired_lease_does_not_allow_background_location() {
        let mut broker = CapabilityBroker::default();
        broker
            .grant(
                CapabilityLease {
                    uid: 10042,
                    resource: Resource::Location,
                    scope: CapabilityScope::Navigation,
                    origin: CapabilityOrigin::TrustedSystemSignal,
                    issued_monotonic_ns: 1,
                    ttl_ms: 1,
                },
                1,
            )
            .unwrap();

        let mut guard = ApplicationGuard::new(broker, RecordingAdvisor::default());
        let req = request(Resource::Location);
        assert_eq!(
            guard.evaluate(req),
            GuardDecision::Deny(Reason::HardBackgroundSensorInvariant)
        );
    }

    #[test]
    fn ambiguous_access_can_receive_bounded_model_lease() {
        let advisor = RecordingAdvisor {
            advice: Some(ModelAdvice::AllowTemporary {
                scope: CapabilityScope::ConnectedDevice,
                ttl_ms: 60_000,
            }),
            ..RecordingAdvisor::default()
        };
        let mut guard = ApplicationGuard::new(CapabilityBroker::default(), advisor);

        assert_eq!(
            guard.evaluate(request(Resource::BluetoothScan)),
            GuardDecision::AllowTemporary {
                scope: CapabilityScope::ConnectedDevice,
                ttl_ms: 60_000
            }
        );
        assert!(guard
            .broker()
            .has_active(10042, Resource::BluetoothScan, 1_000_000_001));
    }

    #[test]
    fn model_abstention_preserves_platform_compatibility() {
        let mut guard =
            ApplicationGuard::new(CapabilityBroker::default(), RecordingAdvisor::default());
        assert_eq!(
            guard.evaluate(request(Resource::Identity)),
            GuardDecision::DeferToPlatform
        );
    }

    #[test]
    fn network_resource_is_reserved_for_network_guard() {
        let mut guard =
            ApplicationGuard::new(CapabilityBroker::default(), RecordingAdvisor::default());
        assert_eq!(
            guard.evaluate(request(Resource::NetworkEgress)),
            GuardDecision::UnsupportedResource
        );
    }
}
