// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_android_bridge::{
    apply_trusted_signal, apply_uid_foreground_baseline, enable_background_override,
    restore_foreground_baseline, AppOpsBackend, TrustedSignal, APP_GUARD_RESOURCES,
};
use nasaru_capability::{BrokerError, CapabilityBroker};
use nasaru_policy::Resource;
use nasaru_signal_state::{SignalEffect, SignalEvent, SignalState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEffect {
    ThirdPartyBaselineApplied { uid: u32 },
    TrustedSessionApplied(TrustedSignal),
    ModelEvaluationRequested { uid: u32 },
    UidRemoved { uid: u32 },
}

#[derive(Debug)]
pub enum RuntimeError<E> {
    Broker(BrokerError),
    Backend(E),
    BackendRollback(E),
}

#[derive(Debug)]
pub struct M1Runtime<B> {
    signals: SignalState,
    broker: CapabilityBroker,
    backend: B,
}

impl<B: AppOpsBackend> M1Runtime<B> {
    pub fn new(backend: B) -> Self {
        Self {
            signals: SignalState::default(),
            broker: CapabilityBroker::default(),
            backend,
        }
    }

    pub fn with_state(
        backend: B,
        signals: SignalState,
        broker: CapabilityBroker,
    ) -> Self {
        Self {
            signals,
            broker,
            backend,
        }
    }

    pub fn broker(&self) -> &CapabilityBroker {
        &self.broker
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    pub fn next_expiry_ns(&self) -> Option<u64> {
        self.broker.next_expiry_ns()
    }

    pub fn handle_event(
        &mut self,
        event: SignalEvent,
    ) -> Result<Vec<RuntimeEffect>, RuntimeError<B::Error>> {
        self.expire(event.monotonic_ns())?;

        let mut out = Vec::new();
        for effect in self.signals.ingest(event) {
            self.apply_effect(effect, event.monotonic_ns(), &mut out)?;
        }
        Ok(out)
    }

    pub fn expire(
        &mut self,
        now_monotonic_ns: u64,
    ) -> Result<usize, RuntimeError<B::Error>> {
        let snapshot = self.broker.clone();
        let expired = self.broker.expire_collect(now_monotonic_ns);
        let expired_count = expired.len();
        let mut restored = Vec::<(u32, Resource)>::new();

        for lease in expired {
            let key = (lease.uid, lease.resource);
            if restored.contains(&key) {
                continue;
            }
            if APP_GUARD_RESOURCES.contains(&lease.resource)
                && !self
                    .broker
                    .has_active(lease.uid, lease.resource, now_monotonic_ns)
            {
                if let Err(error) =
                    restore_foreground_baseline(&mut self.backend, lease.uid, lease.resource)
                {
                    self.broker = snapshot.clone();
                    if let Err(rollback_error) =
                        self.rollback_expiry_appops(&snapshot, &restored, now_monotonic_ns)
                    {
                        return Err(RuntimeError::BackendRollback(rollback_error));
                    }
                    return Err(RuntimeError::Backend(error));
                }
                restored.push(key);
            }
        }

        Ok(expired_count)
    }

    fn apply_effect(
        &mut self,
        effect: SignalEffect,
        now_monotonic_ns: u64,
        out: &mut Vec<RuntimeEffect>,
    ) -> Result<(), RuntimeError<B::Error>> {
        match effect {
            SignalEffect::ApplyThirdPartyBaseline { uid } => {
                apply_uid_foreground_baseline(&mut self.backend, uid)
                    .map_err(RuntimeError::Backend)?;
                out.push(RuntimeEffect::ThirdPartyBaselineApplied { uid });
            }
            SignalEffect::RemoveUidState { uid } => {
                self.broker.revoke_uid(uid);
                out.push(RuntimeEffect::UidRemoved { uid });
            }
            SignalEffect::TrustedSession(signal) => {
                self.apply_session(signal, now_monotonic_ns)?;
                out.push(RuntimeEffect::TrustedSessionApplied(signal));
            }
            SignalEffect::AmbiguousContextChanged { uid } => {
                out.push(RuntimeEffect::ModelEvaluationRequested { uid });
            }
        }
        Ok(())
    }

    fn apply_session(
        &mut self,
        signal: TrustedSignal,
        now_monotonic_ns: u64,
    ) -> Result<(), RuntimeError<B::Error>> {
        let uid = signal_uid(signal);
        let snapshot = self.broker.clone();
        let before = APP_GUARD_RESOURCES
            .map(|resource| snapshot.has_active(uid, resource, now_monotonic_ns));

        apply_trusted_signal(&mut self.broker, signal, now_monotonic_ns)
            .map_err(RuntimeError::Broker)?;

        let mut changed = Vec::<Resource>::new();

        for (index, resource) in APP_GUARD_RESOURCES.into_iter().enumerate() {
            let after = self.broker.has_active(uid, resource, now_monotonic_ns);
            if before[index] == after {
                continue;
            }

            let result = if after {
                enable_background_override(&mut self.backend, uid, resource)
            } else {
                restore_foreground_baseline(&mut self.backend, uid, resource)
            };

            if let Err(error) = result {
                self.broker = snapshot.clone();
                if let Err(rollback_error) =
                    self.rollback_session_appops(&snapshot, uid, &changed, now_monotonic_ns)
                {
                    return Err(RuntimeError::BackendRollback(rollback_error));
                }
                return Err(RuntimeError::Backend(error));
            }

            changed.push(resource);
        }

        Ok(())
    }

    fn rollback_session_appops(
        &mut self,
        snapshot: &CapabilityBroker,
        uid: u32,
        resources: &[Resource],
        now_monotonic_ns: u64,
    ) -> Result<(), B::Error> {
        for &resource in resources {
            if snapshot.has_active(uid, resource, now_monotonic_ns) {
                enable_background_override(&mut self.backend, uid, resource)?;
            } else {
                restore_foreground_baseline(&mut self.backend, uid, resource)?;
            }
        }
        Ok(())
    }

    fn rollback_expiry_appops(
        &mut self,
        snapshot: &CapabilityBroker,
        resources: &[(u32, Resource)],
        now_monotonic_ns: u64,
    ) -> Result<(), B::Error> {
        for &(uid, resource) in resources {
            if snapshot.has_active(uid, resource, now_monotonic_ns) {
                enable_background_override(&mut self.backend, uid, resource)?;
            } else {
                restore_foreground_baseline(&mut self.backend, uid, resource)?;
            }
        }
        Ok(())
    }
}

fn signal_uid(signal: TrustedSignal) -> u32 {
    match signal {
        TrustedSignal::NavigationStarted { uid }
        | TrustedSignal::NavigationStopped { uid }
        | TrustedSignal::CompanionHealthSyncStarted { uid }
        | TrustedSignal::CompanionHealthSyncStopped { uid }
        | TrustedSignal::VoiceSessionStarted { uid }
        | TrustedSignal::VoiceSessionStopped { uid }
        | TrustedSignal::VideoSessionStarted { uid }
        | TrustedSignal::VideoSessionStopped { uid }
        | TrustedSignal::FileTransferStarted { uid }
        | TrustedSignal::FileTransferStopped { uid } => uid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nasaru_android_bridge::{AppOpChange, AppOpMode};
    use nasaru_signal_state::{ActiveOp, FgsTypes, UidImportance};

    #[derive(Debug, Default)]
    struct FakeBackend {
        changes: Vec<AppOpChange>,
        fail_after: Option<usize>,
    }

    impl AppOpsBackend for FakeBackend {
        type Error = &'static str;

        fn set_mode(&mut self, change: AppOpChange) -> Result<(), Self::Error> {
            if self.fail_after == Some(self.changes.len()) {
                return Err("injected backend failure");
            }
            self.changes.push(change);
            Ok(())
        }
    }

    #[test]
    fn bootstrap_uid_applies_foreground_baseline() {
        let mut runtime = M1Runtime::new(FakeBackend::default());
        let effects = runtime
            .handle_event(SignalEvent::BootstrapUid {
                uid: 20000,
                third_party: true,
                monotonic_ns: 1,
            })
            .unwrap();

        assert_eq!(
            effects,
            vec![RuntimeEffect::ThirdPartyBaselineApplied { uid: 20000 }]
        );
        assert_eq!(runtime.backend().changes.len(), 4);
        assert!(runtime
            .backend()
            .changes
            .iter()
            .all(|change| change.mode == AppOpMode::Foreground));
    }

    #[test]
    fn navigation_composite_enables_and_disables_location_override() {
        let mut runtime = M1Runtime::new(FakeBackend::default());

        for event in [
            SignalEvent::UidImportanceChanged {
                uid: 42,
                importance: UidImportance::Foreground,
                monotonic_ns: 1,
            },
            SignalEvent::ForegroundServiceTypesChanged {
                uid: 42,
                types: FgsTypes::LOCATION,
                monotonic_ns: 2,
            },
            SignalEvent::AppOpActiveChanged {
                uid: 42,
                op: ActiveOp::Location,
                active: true,
                monotonic_ns: 3,
            },
        ] {
            runtime.handle_event(event).unwrap();
        }

        assert!(runtime.broker().has_active(42, Resource::Location, 4));
        assert!(runtime
            .backend()
            .changes
            .iter()
            .rev()
            .take(2)
            .all(|change| change.mode == AppOpMode::Allowed));

        runtime
            .handle_event(SignalEvent::AppOpActiveChanged {
                uid: 42,
                op: ActiveOp::Location,
                active: false,
                monotonic_ns: 5,
            })
            .unwrap();

        assert!(!runtime.broker().has_active(42, Resource::Location, 6));
        assert!(runtime
            .backend()
            .changes
            .iter()
            .rev()
            .take(2)
            .all(|change| change.mode == AppOpMode::Foreground));
    }

    #[test]
    fn session_backend_failure_rolls_back_broker() {
        let backend = FakeBackend {
            changes: Vec::new(),
            fail_after: Some(0),
        };
        let mut runtime = M1Runtime::new(backend);

        runtime
            .handle_event(SignalEvent::UidImportanceChanged {
                uid: 51,
                importance: UidImportance::Foreground,
                monotonic_ns: 1,
            })
            .unwrap();
        runtime
            .handle_event(SignalEvent::ForegroundServiceTypesChanged {
                uid: 51,
                types: FgsTypes::LOCATION,
                monotonic_ns: 2,
            })
            .unwrap();

        let result = runtime.handle_event(SignalEvent::AppOpActiveChanged {
            uid: 51,
            op: ActiveOp::Location,
            active: true,
            monotonic_ns: 3,
        });

        assert!(matches!(result, Err(RuntimeError::Backend(_))));
        assert!(!runtime.broker().has_active(51, Resource::Location, 4));
        assert!(!runtime.broker().has_active(51, Resource::NetworkEgress, 4));
    }

    #[test]
    fn expiry_restores_foreground_mode_once_per_resource() {
        let mut broker = CapabilityBroker::default();
        apply_trusted_signal(
            &mut broker,
            TrustedSignal::NavigationStarted { uid: 9 },
            0,
        )
        .unwrap();

        let mut runtime = M1Runtime::with_state(
            FakeBackend::default(),
            SignalState::default(),
            broker,
        );

        assert_eq!(runtime.expire(120_000_000_000).unwrap(), 2);
        assert!(!runtime
            .broker()
            .has_active(9, Resource::Location, 120_000_000_000));
        assert_eq!(runtime.backend().changes.len(), 2);
        assert!(runtime
            .backend()
            .changes
            .iter()
            .all(|change| change.mode == AppOpMode::Foreground));
    }

    #[test]
    fn ambiguous_signal_requests_model_without_appops_change() {
        let mut runtime = M1Runtime::new(FakeBackend::default());
        let effects = runtime
            .handle_event(SignalEvent::ForegroundServiceTypesChanged {
                uid: 44,
                types: FgsTypes::DATA_SYNC,
                monotonic_ns: 1,
            })
            .unwrap();

        assert_eq!(
            effects,
            vec![RuntimeEffect::ModelEvaluationRequested { uid: 44 }]
        );
        assert!(runtime.backend().changes.is_empty());
    }
}
