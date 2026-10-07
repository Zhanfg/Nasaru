// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_android_shell_backend::ShellAppOpsBackend;
use nasaru_local_transport::{
    PeerPolicy, SeqPacketListener, TransportError, DEFAULT_MAX_PACKET,
};
use nasaru_riksu::{RiksuError, RiksuHeader};
use nasaru_runtime::{M1Runtime, RuntimeEffect, RuntimeError};
use nasaru_signal_wire::{decode_signal_event, WireError};

#[derive(Debug, Default)]
pub struct CollectorSession {
    last_sequence: Option<u64>,
}

impl CollectorSession {
    pub fn last_sequence(&self) -> Option<u64> {
        self.last_sequence
    }
}

#[derive(Debug)]
pub enum DaemonError<E> {
    Riksu(RiksuError),
    Wire(WireError),
    ReplayOrOutOfOrder { last: u64, got: u64 },
    Runtime(RuntimeError<E>),
}

#[derive(Debug)]
pub enum ServeError<E> {
    Transport(TransportError),
    Daemon(DaemonError<E>),
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
        session: &mut CollectorSession,
        frame: &[u8],
    ) -> Result<Vec<RuntimeEffect>, DaemonError<B::Error>> {
        let header = RiksuHeader::decode(frame).map_err(DaemonError::Riksu)?;
        if let Some(last) = session.last_sequence {
            if header.sequence <= last {
                return Err(DaemonError::ReplayOrOutOfOrder {
                    last,
                    got: header.sequence,
                });
            }
        }

        let event = decode_signal_event(frame).map_err(DaemonError::Wire)?;
        let effects = self
            .runtime
            .handle_event(event)
            .map_err(DaemonError::Runtime)?;
        session.last_sequence = Some(header.sequence);
        Ok(effects)
    }

    pub fn serve_one_collector(
        &mut self,
        listener: &SeqPacketListener,
        policy: &PeerPolicy,
    ) -> Result<usize, ServeError<B::Error>> {
        let connection = listener.accept(policy).map_err(ServeError::Transport)?;
        let mut session = CollectorSession::default();
        let mut processed = 0usize;

        while let Some(frame) = connection
            .recv_packet(DEFAULT_MAX_PACKET)
            .map_err(ServeError::Transport)?
        {
            self.process_frame(&mut session, &frame)
                .map_err(ServeError::Daemon)?;
            processed = processed.saturating_add(1);
        }

        Ok(processed)
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
        let mut session = CollectorSession::default();

        let effects = daemon.process_frame(&mut session, &frame).unwrap();
        assert_eq!(
            effects,
            vec![RuntimeEffect::ThirdPartyBaselineApplied { uid: 20001 }]
        );
        assert_eq!(session.last_sequence(), Some(1));
    }

    #[test]
    fn duplicate_sequence_is_rejected_before_runtime() {
        let event = SignalEvent::PackageAdded {
            uid: 20002,
            third_party: true,
            monotonic_ns: 88,
        };
        let frame = encode_signal_event(7, event).unwrap();
        let mut daemon = NasaruDaemon::new(FakeBackend::default());
        let mut session = CollectorSession::default();

        daemon.process_frame(&mut session, &frame).unwrap();
        let error = daemon.process_frame(&mut session, &frame).unwrap_err();
        assert!(matches!(
            error,
            DaemonError::ReplayOrOutOfOrder { last: 7, got: 7 }
        ));
    }

    #[test]
    fn malformed_frame_is_rejected_before_runtime() {
        let mut daemon = NasaruDaemon::new(FakeBackend::default());
        let mut session = CollectorSession::default();
        let error = daemon.process_frame(&mut session, b"bad").unwrap_err();
        assert!(matches!(error, DaemonError::Riksu(_)));
        assert_eq!(session.last_sequence(), None);
    }
}
