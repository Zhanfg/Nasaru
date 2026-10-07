// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_android_bridge::{
    apply_trusted_signal, enable_background_override, reconcile_expired,
    restore_foreground_baseline, AppOpsBackend, TrustedSignal,
};
use nasaru_capability::{BrokerError, CapabilityBroker};
use nasaru_policy::Resource;
use nasaru_signal_state::{SignalEffect, SignalEvent, SignalState};

const APPOPS_RESOURCES: [Resource; 3] =
    [Resource::Camera, Resource::Microphone, Resource::Location];

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

    pub fn broker(&self) -> &CapabilityBroker {
        &self.broker
    }

    pub fn backend(&self) -> &B {
        &self.backend
    }

    pub fn handle_event(
        &mut self,
        event: SignalEvent,
    ) -> Result<Vec<RuntimeEffect>, RuntimeError<B::Error>> {
        let mut out = Vec::new();
        for effect in self.signals.ingest(event) {
            self.apply_effect(effect, event.monotonic_ns(), &mut out)?;
        }
        Ok(out)
    }

    pub fn expire(&mut self, now_monotonic_ns: u64) -> Result<usize, RuntimeError<B::Error>> {
        reconcile_expired(&mut self.broker, &mut self.backend, now_monotonic_ns)
            .map_err(RuntimeError::Backend)
    }

    pub fn next_expiry_ns(&self) -> Option<u64> {
        self.broker.next_expiry_ns()
    }

    fn apply_effect(
        &mut self,
        effect: SignalEffect,
        now_monotonic_ns: u64,
        out: &mut Vec<RuntimeEffect>,
    ) -> Result<(), RuntimeError<B::Error>> {
        match effect {
            SignalEffect::ApplyThirdPartyBaseline { uid } => {
                for resource in APPOPS_RESOURCES {
                    restore_foreground_baseline(&mut self.backend, uid, resource)
                        .map_err(RuntimeError::Backend)?;
                }
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
        let before = APPOPS_RESOURCES.map(|resource| {
            self.broker
                .has_active(uid, resource, now_monotonic_ns)
        });

        apply_trusted_signal(&mut self.broker, signal, now_monotonic_ns)
            .map_err(RuntimeError::Broker)?;

        for (index, resource) in APPOPS_RESOURCES.into_iter().enumerate() {
            let after = self
                .broker
                .has_active(uid, resource, now_monotonic_ns);
            match (before[index], after) {
                (false, true) => {
                    enable_background_override(&mut self.backend, uid, resource)
                        .map_err(RuntimeError::Backend)?;
                }
                (true, false) => {
                    restore_foreground_baseline(&mut self.backend, uid, resource)
                        .map_err(RuntimeError::Backend)?;
                }
                _ => {}
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
    }

    impl AppOpsBackend for FakeBackend {
        type Error = ();

        fn set_mode(&mut self, change: AppOpChange) -> Result<(), Self::Error> {
            self.changes.push(change);
            Ok(())
        }
    }

    #[test]
    fn third_party_install_applies_foreground_baseline() {
        let mut runtime = M1Runtime::new(FakeBackend::default());
        let effects = runtime
            .handle_event(SignalEvent::PackageAdded {
                uid: 20001,
                third_party: true,
                monotonic_ns: 1,
            })
            .unwrap();

        assert_eq!(
            effects,
            vec![RuntimeEffect::ThirdPartyBaselineApplied { uid: 20001 }]
        );
        assert_eq!(runtime.backend().changes.len(), 4);
        assert!(runtime
            .backend()
            .changes
            .iter()
            .all(|change| change.mode == AppOpMode::Foreground));
    }

    #[test]
    fn navigation_composite_enables_location_override() {
        let mut runtime = M1Runtime::new(FakeBackend::default());

        runtime
            .handle_event(SignalEvent::UidImportanceChanged {
                uid: 42,
                importance: UidImportance::Foreground,
                monotonic_ns: 1,
            })
            .unwrap();
        runtime
            .handle_event(SignalEvent::ForegroundServiceTypesChanged {
                uid: 42,
                types: FgsTypes::LOCATION,
                monotonic_ns: 2,
            })
            .unwrap();
        let effects = runtime
            .handle_event(SignalEvent::AppOpActiveChanged {
                uid: 42,
                op: ActiveOp::Location,
                active: true,
                monotonic_ns: 3,
            })
            .unwrap();

        assert!(effects.contains(&RuntimeEffect::TrustedSessionApplied(
            TrustedSignal::NavigationStarted { uid: 42 }
        )));
        assert!(runtime.broker().has_active(42, Resource::Location, 4));
        assert!(runtime
            .backend()
            .changes
            .iter()
            .any(|change| change.uid == 42 && change.mode == AppOpMode::Allowed));
    }

    #[test]
    fn navigation_stop_restores_location_foreground_mode() {
        let mut runtime = M1Runtime::new(FakeBackend::default());
        for event in [
            SignalEvent::UidImportanceChanged {
                uid: 43,
                importance: UidImportance::Foreground,
                monotonic_ns: 1,
            },
            SignalEvent::ForegroundServiceTypesChanged {
                uid: 43,
                types: FgsTypes::LOCATION,
                monotonic_ns: 2,
            },
            SignalEvent::AppOpActiveChanged {
                uid: 43,
                op: ActiveOp::Location,
                active: true,
                monotonic_ns: 3,
            },
        ] {
            runtime.handle_event(event).unwrap();
        }

        runtime.backend.changes.clear();
        runtime
            .handle_event(SignalEvent::AppOpActiveChanged {
                uid: 43,
                op: ActiveOp::Location,
                active: false,
                monotonic_ns: 4,
            })
            .unwrap();

        assert!(!runtime.broker().has_active(43, Resource::Location, 5));
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
