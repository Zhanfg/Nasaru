// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_capability::{
    BrokerError, CapabilityBroker, CapabilityLease, CapabilityOrigin, CapabilityScope,
};
use nasaru_policy::Resource;

pub const NAVIGATION_TTL_MS: u32 = 120_000;
pub const COMPANION_TTL_MS: u32 = 120_000;
pub const VOICE_TTL_MS: u32 = 120_000;
pub const TRANSFER_TTL_MS: u32 = 300_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustedSignal {
    NavigationStarted { uid: u32 },
    NavigationStopped { uid: u32 },
    CompanionHealthSyncStarted { uid: u32 },
    CompanionHealthSyncStopped { uid: u32 },
    VoiceSessionStarted { uid: u32, video: bool },
    VoiceSessionStopped { uid: u32 },
    FileTransferStarted { uid: u32 },
    FileTransferStopped { uid: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppOpClass {
    Camera,
    RecordAudio,
    FineLocation,
    CoarseLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppOpMode {
    Foreground,
    Allowed,
    Ignored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppOpChange {
    pub uid: u32,
    pub op: AppOpClass,
    pub mode: AppOpMode,
}

pub trait AppOpsBackend {
    type Error;

    fn set_mode(&mut self, change: AppOpChange) -> Result<(), Self::Error>;
}

pub fn foreground_baseline(resource: Resource) -> Option<&'static [AppOpClass]> {
    const CAMERA: &[AppOpClass] = &[AppOpClass::Camera];
    const MICROPHONE: &[AppOpClass] = &[AppOpClass::RecordAudio];
    const LOCATION: &[AppOpClass] = &[AppOpClass::FineLocation, AppOpClass::CoarseLocation];

    match resource {
        Resource::Camera => Some(CAMERA),
        Resource::Microphone => Some(MICROPHONE),
        Resource::Location => Some(LOCATION),
        _ => None,
    }
}

pub fn apply_trusted_signal(
    broker: &mut CapabilityBroker,
    signal: TrustedSignal,
    now_monotonic_ns: u64,
) -> Result<(), BrokerError> {
    match signal {
        TrustedSignal::NavigationStarted { uid } => grant_many(
            broker,
            uid,
            CapabilityScope::Navigation,
            NAVIGATION_TTL_MS,
            &[Resource::Location, Resource::NetworkEgress],
            now_monotonic_ns,
        ),
        TrustedSignal::NavigationStopped { uid } => {
            broker.revoke(uid, Resource::Location);
            broker.revoke(uid, Resource::NetworkEgress);
            Ok(())
        }
        TrustedSignal::CompanionHealthSyncStarted { uid } => grant_many(
            broker,
            uid,
            CapabilityScope::HealthSync,
            COMPANION_TTL_MS,
            &[Resource::BluetoothScan, Resource::NetworkEgress],
            now_monotonic_ns,
        ),
        TrustedSignal::CompanionHealthSyncStopped { uid } => {
            broker.revoke(uid, Resource::BluetoothScan);
            broker.revoke(uid, Resource::NetworkEgress);
            Ok(())
        }
        TrustedSignal::VoiceSessionStarted { uid, video } => {
            let resources: &[Resource] = if video {
                &[
                    Resource::Microphone,
                    Resource::Camera,
                    Resource::NetworkEgress,
                ]
            } else {
                &[Resource::Microphone, Resource::NetworkEgress]
            };
            grant_many(
                broker,
                uid,
                CapabilityScope::VoiceSession,
                VOICE_TTL_MS,
                resources,
                now_monotonic_ns,
            )
        }
        TrustedSignal::VoiceSessionStopped { uid } => {
            broker.revoke(uid, Resource::Microphone);
            broker.revoke(uid, Resource::Camera);
            broker.revoke(uid, Resource::NetworkEgress);
            Ok(())
        }
        TrustedSignal::FileTransferStarted { uid } => grant_many(
            broker,
            uid,
            CapabilityScope::FileTransfer,
            TRANSFER_TTL_MS,
            &[Resource::SensitiveFile, Resource::NetworkEgress],
            now_monotonic_ns,
        ),
        TrustedSignal::FileTransferStopped { uid } => {
            broker.revoke(uid, Resource::SensitiveFile);
            broker.revoke(uid, Resource::NetworkEgress);
            Ok(())
        }
    }
}

pub fn enable_background_override<B: AppOpsBackend>(
    backend: &mut B,
    uid: u32,
    resource: Resource,
) -> Result<(), B::Error> {
    if let Some(ops) = foreground_baseline(resource) {
        for &op in ops {
            backend.set_mode(AppOpChange {
                uid,
                op,
                mode: AppOpMode::Allowed,
            })?;
        }
    }
    Ok(())
}

pub fn restore_foreground_baseline<B: AppOpsBackend>(
    backend: &mut B,
    uid: u32,
    resource: Resource,
) -> Result<(), B::Error> {
    if let Some(ops) = foreground_baseline(resource) {
        for &op in ops {
            backend.set_mode(AppOpChange {
                uid,
                op,
                mode: AppOpMode::Foreground,
            })?;
        }
    }
    Ok(())
}

fn grant_many(
    broker: &mut CapabilityBroker,
    uid: u32,
    scope: CapabilityScope,
    ttl_ms: u32,
    resources: &[Resource],
    now_monotonic_ns: u64,
) -> Result<(), BrokerError> {
    for &resource in resources {
        broker.grant(
            CapabilityLease {
                uid,
                resource,
                scope,
                origin: CapabilityOrigin::TrustedSystemSignal,
                issued_monotonic_ns: now_monotonic_ns,
                ttl_ms,
            },
            now_monotonic_ns,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct FakeAppOps {
        changes: Vec<AppOpChange>,
    }

    impl AppOpsBackend for FakeAppOps {
        type Error = ();

        fn set_mode(&mut self, change: AppOpChange) -> Result<(), Self::Error> {
            self.changes.push(change);
            Ok(())
        }
    }

    #[test]
    fn navigation_only_grants_expected_capabilities() {
        let mut broker = CapabilityBroker::default();
        apply_trusted_signal(
            &mut broker,
            TrustedSignal::NavigationStarted { uid: 42 },
            1_000,
        )
        .unwrap();

        assert!(broker.has_active(42, Resource::Location, 2_000));
        assert!(broker.has_active(42, Resource::NetworkEgress, 2_000));
        assert!(!broker.has_active(42, Resource::Camera, 2_000));
    }

    #[test]
    fn stop_signal_revokes_session_capabilities() {
        let mut broker = CapabilityBroker::default();
        apply_trusted_signal(
            &mut broker,
            TrustedSignal::CompanionHealthSyncStarted { uid: 7 },
            0,
        )
        .unwrap();
        apply_trusted_signal(
            &mut broker,
            TrustedSignal::CompanionHealthSyncStopped { uid: 7 },
            1,
        )
        .unwrap();

        assert!(!broker.has_active(7, Resource::BluetoothScan, 2));
        assert!(!broker.has_active(7, Resource::NetworkEgress, 2));
    }

    #[test]
    fn video_call_grants_camera_and_microphone() {
        let mut broker = CapabilityBroker::default();
        apply_trusted_signal(
            &mut broker,
            TrustedSignal::VoiceSessionStarted {
                uid: 8,
                video: true,
            },
            0,
        )
        .unwrap();

        assert!(broker.has_active(8, Resource::Camera, 1));
        assert!(broker.has_active(8, Resource::Microphone, 1));
    }

    #[test]
    fn appops_override_is_bounded_to_mapped_sensitive_ops() {
        let mut backend = FakeAppOps::default();
        enable_background_override(&mut backend, 99, Resource::Location).unwrap();
        assert_eq!(
            backend.changes,
            vec![
                AppOpChange {
                    uid: 99,
                    op: AppOpClass::FineLocation,
                    mode: AppOpMode::Allowed
                },
                AppOpChange {
                    uid: 99,
                    op: AppOpClass::CoarseLocation,
                    mode: AppOpMode::Allowed
                }
            ]
        );

        backend.changes.clear();
        restore_foreground_baseline(&mut backend, 99, Resource::Location).unwrap();
        assert!(backend
            .changes
            .iter()
            .all(|change| change.mode == AppOpMode::Foreground));
    }

    #[test]
    fn files_do_not_pretend_to_be_an_appops_resource() {
        assert_eq!(foreground_baseline(Resource::SensitiveFile), None);
    }
}
