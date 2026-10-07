// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_android_bridge::{
    apply_trusted_signal, apply_uid_foreground_baseline, enable_background_override,
    restore_foreground_baseline, AppOpsBackend, TrustedSignal, APP_GUARD_RESOURCES,
};
use nasaru_capability::{BrokerError, CapabilityBroker};
use nasaru_policy::Resource;
use nasaru_signal_state::{SignalEffect, SignalEvent, SignalState};

#[derive(Debug)]
pub enum EngineError<E> {
    Broker(BrokerError),
    AppOps(E),
    AppOpsRollback(E),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EngineReport {
    pub effects: usize,
    pub baseline_updates: usize,
    pub session_transitions: usize,
    pub expired_leases: usize,
}

#[derive(Debug)]
pub struct M1Engine<B> {
    signals: SignalState,
    broker: CapabilityBroker,
    appops: B,
}

impl<B: AppOpsBackend> M1Engine<B> {
    pub fn new(appops: B) -> Self {
        Self {
            signals: SignalState::default(),
            broker: CapabilityBroker::default(),
            appops,
        }
    }

    pub fn with_state(appops: B, signals: SignalState, broker: CapabilityBroker) -> Self {
        Self {
            signals,
            broker,
            appops,
        }
    }

    pub fn broker(&self) -> &CapabilityBroker {
        &self.broker
    }

    pub fn signals(&self) -> &SignalState {
        &self.signals
    }

    pub fn appops(&self) -> &B {
        &self.appops
    }

    pub fn appops_mut(&mut self) -> &mut B {
        &mut self.appops
    }

    pub fn next_expiry_ns(&self) -> Option<u64> {
        self.broker.next_expiry_ns()
    }

    pub fn ingest(&mut self, event: SignalEvent) -> Result<EngineReport, EngineError<B::Error>> {
        let now_ns = event.monotonic_ns();
        let mut report = self.reconcile_expired(now_ns)?;
        let effects = self.signals.ingest(event);
        report.effects += effects.len();

        for effect in effects {
            match effect {
                SignalEffect::ApplyThirdPartyBaseline { uid } => {
                    apply_uid_foreground_baseline(&mut self.appops, uid)
                        .map_err(EngineError::AppOps)?;
                    report.baseline_updates += 1;
                }
                SignalEffect::RemoveUidState { uid } => {
                    self.broker.revoke_uid(uid);
                }
                SignalEffect::TrustedSession(signal) => {
                    self.apply_session(signal, now_ns)?;
                    report.session_transitions += 1;
                }
                SignalEffect::AmbiguousContextChanged { .. } => {}
            }
        }

        Ok(report)
    }

    pub fn tick(&mut self, now_monotonic_ns: u64) -> Result<EngineReport, EngineError<B::Error>> {
        self.reconcile_expired(now_monotonic_ns)
    }

    fn reconcile_expired(
        &mut self,
        now_monotonic_ns: u64,
    ) -> Result<EngineReport, EngineError<B::Error>> {
        let expired = self.broker.expire_collect(now_monotonic_ns);
        let mut report = EngineReport {
            expired_leases: expired.len(),
            ..EngineReport::default()
        };

        for lease in expired {
            if foreground_managed(lease.resource)
                && !self
                    .broker
                    .has_active(lease.uid, lease.resource, now_monotonic_ns)
            {
                restore_foreground_baseline(&mut self.appops, lease.uid, lease.resource)
                    .map_err(EngineError::AppOps)?;
                report.baseline_updates += 1;
            }
        }

        Ok(report)
    }

    fn apply_session(
        &mut self,
        signal: TrustedSignal,
        now_monotonic_ns: u64,
    ) -> Result<(), EngineError<B::Error>> {
        let affected = session_resources(signal);
        let before = self.broker.clone();

        apply_trusted_signal(&mut self.broker, signal, now_monotonic_ns)
            .map_err(EngineError::Broker)?;

        let mut changed_resources = Vec::new();
        for &resource in affected {
            if !foreground_managed(resource) {
                continue;
            }

            let was_active = before.has_active(signal_uid(signal), resource, now_monotonic_ns);
            let is_active = self
                .broker
                .has_active(signal_uid(signal), resource, now_monotonic_ns);

            if was_active == is_active {
                continue;
            }

            let result = if is_active {
                enable_background_override(&mut self.appops, signal_uid(signal), resource)
            } else {
                restore_foreground_baseline(&mut self.appops, signal_uid(signal), resource)
            };

            if let Err(error) = result {
                self.broker = before.clone();
                if let Err(rollback_error) = self.restore_appops_from_snapshot(
                    &before,
                    signal_uid(signal),
                    &changed_resources,
                    now_monotonic_ns,
                ) {
                    return Err(EngineError::AppOpsRollback(rollback_error));
                }
                return Err(EngineError::AppOps(error));
            }

            changed_resources.push(resource);
        }

        Ok(())
    }

    fn restore_appops_from_snapshot(
        &mut self,
        snapshot: &CapabilityBroker,
        uid: u32,
        resources: &[Resource],
        now_monotonic_ns: u64,
    ) -> Result<(), B::Error> {
        for &resource in resources {
            if snapshot.has_active(uid, resource, now_monotonic_ns) {
                enable_background_override(&mut self.appops, uid, resource)?;
            } else {
                restore_foreground_baseline(&mut self.appops, uid, resource)?;
            }
        }
        Ok(())
    }
}

fn foreground_managed(resource: Resource) -> bool {
    APP_GUARD_RESOURCES.contains(&resource)
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

fn session_resources(signal: TrustedSignal) -> &'static [Resource] {
    match signal {
        TrustedSignal::NavigationStarted { .. } | TrustedSignal::NavigationStopped { .. } => {
            &[Resource::Location, Resource::NetworkEgress]
        }
        TrustedSignal::CompanionHealthSyncStarted { .. }
        | TrustedSignal::CompanionHealthSyncStopped { .. } => {
            &[Resource::BluetoothScan, Resource::NetworkEgress]
        }
        TrustedSignal::VoiceSessionStarted { .. } | TrustedSignal::VoiceSessionStopped { .. } => {
            &[Resource::Microphone, Resource::NetworkEgress]
        }
        TrustedSignal::VideoSessionStarted { .. } | TrustedSignal::VideoSessionStopped { .. } => {
            &[Resource::Camera]
        }
        TrustedSignal::FileTransferStarted { .. } | TrustedSignal::FileTransferStopped { .. } => {
            &[Resource::SensitiveFile, Resource::NetworkEgress]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nasaru_android_bridge::{AppOpChange, AppOpMode};
    use nasaru_signal_state::{ActiveOp, FgsTypes, UidImportance};

    #[derive(Debug, Default)]
    struct RecordingAppOps {
        changes: Vec<AppOpChange>,
        fail_after: Option<usize>,
    }

    impl AppOpsBackend for RecordingAppOps {
        type Error = &'static str;

        fn set_mode(&mut self, change: AppOpChange) -> Result<(), Self::Error> {
            if self.fail_after == Some(self.changes.len()) {
                return Err("injected AppOps failure");
            }
            self.changes.push(change);
            Ok(())
        }
    }

    #[test]
    fn bootstrap_third_party_uid_applies_foreground_baseline() {
        let mut engine = M1Engine::new(RecordingAppOps::default());
        let report = engine
            .ingest(SignalEvent::BootstrapUid {
                uid: 20001,
                third_party: true,
                monotonic_ns: 1,
            })
            .unwrap();

        assert_eq!(report.baseline_updates, 1);
        assert_eq!(engine.appops().changes.len(), 4);
        assert!(engine
            .appops()
            .changes
            .iter()
            .all(|change| change.mode == AppOpMode::Foreground));
    }

    #[test]
    fn navigation_session_opens_and_closes_location_override() {
        let mut engine = M1Engine::new(RecordingAppOps::default());

        engine
            .ingest(SignalEvent::UidImportanceChanged {
                uid: 42,
                importance: UidImportance::Foreground,
                monotonic_ns: 1,
            })
            .unwrap();
        engine
            .ingest(SignalEvent::ForegroundServiceTypesChanged {
                uid: 42,
                types: FgsTypes::LOCATION,
                monotonic_ns: 2,
            })
            .unwrap();
        engine
            .ingest(SignalEvent::AppOpActiveChanged {
                uid: 42,
                op: ActiveOp::Location,
                active: true,
                monotonic_ns: 3,
            })
            .unwrap();

        assert!(engine.broker().has_active(42, Resource::Location, 4));
        assert!(engine
            .appops()
            .changes
            .iter()
            .rev()
            .take(2)
            .all(|change| change.mode == AppOpMode::Allowed));

        engine
            .ingest(SignalEvent::AppOpActiveChanged {
                uid: 42,
                op: ActiveOp::Location,
                active: false,
                monotonic_ns: 5,
            })
            .unwrap();

        assert!(!engine.broker().has_active(42, Resource::Location, 6));
        assert!(engine
            .appops()
            .changes
            .iter()
            .rev()
            .take(2)
            .all(|change| change.mode == AppOpMode::Foreground));
    }

    #[test]
    fn single_timer_tick_restores_expired_override() {
        let mut broker = CapabilityBroker::default();
        nasaru_android_bridge::apply_trusted_signal(
            &mut broker,
            TrustedSignal::NavigationStarted { uid: 9 },
            0,
        )
        .unwrap();

        let mut engine =
            M1Engine::with_state(RecordingAppOps::default(), SignalState::default(), broker);
        let report = engine.tick(120_000_000_000).unwrap();

        assert_eq!(report.expired_leases, 2);
        assert!(!engine
            .broker()
            .has_active(9, Resource::Location, 120_000_000_000));
        assert_eq!(engine.appops().changes.len(), 2);
        assert!(engine
            .appops()
            .changes
            .iter()
            .all(|change| change.mode == AppOpMode::Foreground));
    }

    #[test]
    fn backend_failure_rolls_back_broker_state() {
        let appops = RecordingAppOps {
            changes: Vec::new(),
            fail_after: Some(0),
        };
        let mut engine = M1Engine::new(appops);

        engine
            .ingest(SignalEvent::UidImportanceChanged {
                uid: 51,
                importance: UidImportance::Foreground,
                monotonic_ns: 1,
            })
            .unwrap();
        engine
            .ingest(SignalEvent::ForegroundServiceTypesChanged {
                uid: 51,
                types: FgsTypes::LOCATION,
                monotonic_ns: 2,
            })
            .unwrap();

        let result = engine.ingest(SignalEvent::AppOpActiveChanged {
            uid: 51,
            op: ActiveOp::Location,
            active: true,
            monotonic_ns: 3,
        });

        assert!(matches!(result, Err(EngineError::AppOps(_))));
        assert!(!engine.broker().has_active(51, Resource::Location, 4));
        assert!(!engine.broker().has_active(51, Resource::NetworkEgress, 4));
    }
}
