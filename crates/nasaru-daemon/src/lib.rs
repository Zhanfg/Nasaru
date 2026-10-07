// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_android_shell_backend::ShellAppOpsBackend;
use nasaru_runtime::{M1Runtime, RuntimeEffect, RuntimeError};
use nasaru_signal_wire::{decode_signal_event, WireError};

#[derive(Debug)]
pub enum DaemonError<E> {
    Wire(WireError),
    Runtime(RuntimeError<E>),
}

#[derive(Debug)]
pub struct NasaruDaemon<B> {
    runtime: M1Runtime<B>,
}

impl<B> NasaruDaemon<B>
where
    B: nasaru_android_bridge::AppOpsBackend,
{
    pub fn new(backend: B) -> Self {
        Self {
            runtime: M1Runtime::new(backend),
        }
    }

    pub fn process_frame(
        &mut self,
        frame: &[u8],
    ) -> Result<Vec<RuntimeEffect>, DaemonError<B::Error>> {
        let event = decode_signal_event(frame).map_err(DaemonError::Wire)?;
        self.runtime
            .handle_event(event)
            .map_err(DaemonError::Runtime)
    }

    pub fn runtime(&self) -> &M1Runtime<B> {
        &self.runtime
    }
}

impl Default for NasaruDaemon<ShellAppOpsBackend> {
    fn default() -> Self {
        Self::new(ShellAppOpsBackend::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nasaru_android_bridge::{AppOpChange, AppOpsBackend};
    use nasaru_signal_state::SignalEvent;
    use nasaru_signal_wire::encode_signal_event;

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
    fn riksu_package_event_reaches_runtime() {
        let event = SignalEvent::PackageAdded {
            uid: 20001,
            third_party: true,
            monotonic_ns: 88,
        };
        let frame = encode_signal_event(1, event).unwrap();
        let mut daemon = NasaruDaemon::new(FakeBackend::default());

        let effects = daemon.process_frame(&frame).unwrap();
        assert_eq!(
            effects,
            vec![RuntimeEffect::ThirdPartyBaselineApplied { uid: 20001 }]
        );
    }

    #[test]
    fn malformed_frame_is_rejected_before_runtime() {
        let mut daemon = NasaruDaemon::new(FakeBackend::default());
        let error = daemon.process_frame(b"bad").unwrap_err();
        assert!(matches!(error, DaemonError::Wire(_)));
    }
}
