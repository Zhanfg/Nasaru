// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_local_transport::{SeqPacketConnection, TransportError};
use nasaru_signal_state::{SignalEvent, UidImportance};
use nasaru_signal_wire::{encode_signal_event, WireError};
use std::collections::BTreeSet;
use std::ffi::CStr;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::mem::size_of;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Sender;

pub const DEFAULT_CMD_PATH: &str = "/system/bin/cmd";
pub const DEFAULT_PACKAGE_DIR: &str = "/data/system";
pub const DEFAULT_SOCKET_PATH: &str = "/data/adb/nasaru/run/collector.sock";

#[derive(Debug, Clone)]
pub struct CommandPaths {
    pub cmd: PathBuf,
    pub package_dir: PathBuf,
}

impl Default for CommandPaths {
    fn default() -> Self {
        Self {
            cmd: PathBuf::from(DEFAULT_CMD_PATH),
            package_dir: PathBuf::from(DEFAULT_PACKAGE_DIR),
        }
    }
}

#[derive(Debug)]
pub enum CollectorError {
    Io(io::Error),
    Transport(TransportError),
    Wire(WireError),
    CommandFailed(Option<i32>),
    SourceEnded(&'static str),
}

impl From<io::Error> for CollectorError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<TransportError> for CollectorError {
    fn from(value: TransportError) -> Self {
        Self::Transport(value)
    }
}

impl From<WireError> for CollectorError {
    fn from(value: WireError) -> Self {
        Self::Wire(value)
    }
}

#[derive(Debug)]
pub struct CollectorSink {
    connection: SeqPacketConnection,
    sequence: u64,
}

impl CollectorSink {
    pub fn connect(path: impl AsRef<Path>) -> Result<Self, CollectorError> {
        Ok(Self {
            connection: SeqPacketConnection::connect(path)?,
            sequence: 0,
        })
    }

    pub fn send(&mut self, event: SignalEvent) -> Result<(), CollectorError> {
        self.sequence = self.sequence.saturating_add(1);
        let frame = encode_signal_event(self.sequence, event)?;
        self.connection.send_packet(&frame)?;
        Ok(())
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}

pub fn monotonic_ns() -> io::Result<u64> {
    let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
    let result = unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut ts) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((ts.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(ts.tv_nsec as u64))
}

pub fn parse_watch_uids_line(line: &str, now_ns: u64) -> Option<SignalEvent> {
    let mut fields = line.split_whitespace();
    let uid = fields.next()?.parse::<u32>().ok()?;
    let kind = fields.next()?;

    let importance = match kind {
        "procstate" => procstate_to_importance(fields.next()?)?,
        "gone" => UidImportance::Gone,
        "cached" => UidImportance::Cached,
        _ => return None,
    };

    Some(SignalEvent::UidImportanceChanged {
        uid,
        importance,
        monotonic_ns: now_ns,
    })
}

pub fn procstate_to_importance(state: &str) -> Option<UidImportance> {
    let normalized = state.trim();
    match normalized {
        "PER" | "PERU" | "TOP" | "BTOP" | "P" | "PU" | "T" => Some(UidImportance::Foreground),
        "FGS" | "BFGS" | "SF" | "SB" => Some(UidImportance::ForegroundService),
        "IMPF" | "IMPB" | "IF" | "IB" => Some(UidImportance::Visible),
        "TRNB" | "BKUP" | "TPSL" | "BU" | "TS" => Some(UidImportance::Perceptible),
        "SVC" | "RCVR" | "S" | "R" => Some(UidImportance::Service),
        "HVY" | "HOME" | "LAST" | "CAC" | "CACC" | "CRE" | "CEM" | "HO" | "LA" | "CA" | "Ca"
        | "CE" => Some(UidImportance::Cached),
        "N" => Some(UidImportance::Gone),
        _ => None,
    }
}

pub fn parse_third_party_uid_snapshot(output: &str) -> BTreeSet<u32> {
    output
        .lines()
        .filter_map(|line| {
            line.split_whitespace()
                .find_map(|field| field.strip_prefix("uid:"))
                .and_then(|value| value.parse::<u32>().ok())
        })
        .collect()
}

pub fn bootstrap_events(uids: &BTreeSet<u32>, now_ns: u64) -> Vec<SignalEvent> {
    uids.iter()
        .copied()
        .map(|uid| SignalEvent::BootstrapUid {
            uid,
            third_party: true,
            monotonic_ns: now_ns,
        })
        .collect()
}

pub fn snapshot_delta(
    previous: &BTreeSet<u32>,
    next: &BTreeSet<u32>,
    now_ns: u64,
) -> Vec<SignalEvent> {
    let mut events = Vec::new();

    for uid in next.difference(previous).copied() {
        events.push(SignalEvent::PackageAdded {
            uid,
            third_party: true,
            monotonic_ns: now_ns,
        });
    }

    for uid in previous.difference(next).copied() {
        events.push(SignalEvent::PackageRemoved {
            uid,
            replacing: false,
            monotonic_ns: now_ns,
        });
    }

    events
}

pub fn read_third_party_snapshot(paths: &CommandPaths) -> Result<BTreeSet<u32>, CollectorError> {
    let output = Command::new(&paths.cmd)
        .args(["package", "list", "packages", "-3", "-U"])
        .stdin(Stdio::null())
        .output()?;

    if !output.status.success() {
        return Err(CollectorError::CommandFailed(output.status.code()));
    }

    Ok(parse_third_party_uid_snapshot(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

pub fn spawn_uid_watch(
    paths: &CommandPaths,
    tx: Sender<SignalEvent>,
) -> Result<Child, CollectorError> {
    let mut child = Command::new(&paths.cmd)
        .args(["activity", "watch-uids"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or(CollectorError::SourceEnded("watch-uids stdout unavailable"))?;

    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let Ok(line) = line else {
                break;
            };
            let Ok(now_ns) = monotonic_ns() else {
                break;
            };
            if let Some(event) = parse_watch_uids_line(&line, now_ns) {
                if tx.send(event).is_err() {
                    break;
                }
            }
        }
    });

    Ok(child)
}

pub fn spawn_package_watch(
    paths: CommandPaths,
    initial: BTreeSet<u32>,
    tx: Sender<SignalEvent>,
) -> Result<std::thread::JoinHandle<()>, CollectorError> {
    let watcher = PackageWatcher::new(&paths.package_dir)?;
    Ok(std::thread::spawn(move || {
        let mut previous = initial;
        let mut watcher = watcher;

        loop {
            let changed = match watcher.wait_for_package_metadata_change() {
                Ok(changed) => changed,
                Err(_) => break,
            };
            if !changed {
                continue;
            }

            let Ok(next) = read_third_party_snapshot(&paths) else {
                continue;
            };
            let Ok(now_ns) = monotonic_ns() else {
                break;
            };

            for event in snapshot_delta(&previous, &next, now_ns) {
                if tx.send(event).is_err() {
                    return;
                }
            }
            previous = next;
        }
    }))
}

#[derive(Debug)]
struct PackageWatcher {
    fd: OwnedFd,
}

impl PackageWatcher {
    fn new(directory: &Path) -> Result<Self, CollectorError> {
        let raw_fd = unsafe { libc::inotify_init1(libc::IN_CLOEXEC) };
        if raw_fd < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };

        let path =
            std::ffi::CString::new(directory.as_os_str().as_encoded_bytes()).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "package path contains NUL")
            })?;
        let mask = libc::IN_CLOSE_WRITE
            | libc::IN_MOVED_TO
            | libc::IN_CREATE
            | libc::IN_DELETE
            | libc::IN_DELETE_SELF
            | libc::IN_MOVE_SELF;

        let watch = unsafe { libc::inotify_add_watch(fd.as_raw_fd(), path.as_ptr(), mask) };
        if watch < 0 {
            return Err(io::Error::last_os_error().into());
        }

        Ok(Self { fd })
    }

    fn wait_for_package_metadata_change(&mut self) -> Result<bool, CollectorError> {
        let mut buffer = [0u8; 4096];
        let count = unsafe {
            libc::read(
                self.fd.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        };
        if count < 0 {
            return Err(io::Error::last_os_error().into());
        }

        let mut offset = 0usize;
        let count = count as usize;
        while offset + size_of::<libc::inotify_event>() <= count {
            let event = unsafe { &*(buffer.as_ptr().add(offset) as *const libc::inotify_event) };
            let name_start = offset + size_of::<libc::inotify_event>();
            let name_end = name_start.saturating_add(event.len as usize);
            if name_end > count {
                break;
            }

            if event.len > 0 {
                let name_bytes = &buffer[name_start..name_end];
                if let Ok(name) = CStr::from_bytes_until_nul(name_bytes) {
                    if matches!(name.to_bytes(), b"packages.xml" | b"packages.list") {
                        return Ok(true);
                    }
                }
            }

            offset = name_end;
        }

        Ok(false)
    }
}

pub fn run_native_collector(
    socket_path: impl AsRef<Path>,
    paths: CommandPaths,
) -> Result<(), CollectorError> {
    let mut sink = CollectorSink::connect(socket_path)?;
    let initial = read_third_party_snapshot(&paths)?;
    let now_ns = monotonic_ns()?;
    for event in bootstrap_events(&initial, now_ns) {
        sink.send(event)?;
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let _uid_child = spawn_uid_watch(&paths, tx.clone())?;
    let _package_thread = spawn_package_watch(paths, initial, tx.clone())?;
    drop(tx);

    for event in rx {
        sink.send(event)?;
    }

    Err(CollectorError::SourceEnded("all collector sources ended"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_current_aosp_uid_procstates() {
        assert_eq!(
            parse_watch_uids_line("10234 procstate TOP  seq 4", 99),
            Some(SignalEvent::UidImportanceChanged {
                uid: 10234,
                importance: UidImportance::Foreground,
                monotonic_ns: 99
            })
        );
        assert_eq!(
            parse_watch_uids_line("10234 procstate FGS  seq 5", 100),
            Some(SignalEvent::UidImportanceChanged {
                uid: 10234,
                importance: UidImportance::ForegroundService,
                monotonic_ns: 100
            })
        );
        assert_eq!(
            parse_watch_uids_line("10234 gone", 101),
            Some(SignalEvent::UidImportanceChanged {
                uid: 10234,
                importance: UidImportance::Gone,
                monotonic_ns: 101
            })
        );
    }

    #[test]
    fn ignores_watch_uids_prompts_and_non_state_lines() {
        assert_eq!(
            parse_watch_uids_line("Watching uid states... available commands:", 1),
            None
        );
        assert_eq!(parse_watch_uids_line("(q)uit: finish watching", 1), None);
        assert_eq!(parse_watch_uids_line("10234 active", 1), None);
    }

    #[test]
    fn supports_legacy_procstate_tokens_without_changing_signal_abi() {
        assert_eq!(
            procstate_to_importance("T"),
            Some(UidImportance::Foreground)
        );
        assert_eq!(
            procstate_to_importance("SF"),
            Some(UidImportance::ForegroundService)
        );
        assert_eq!(procstate_to_importance("CE"), Some(UidImportance::Cached));
    }

    #[test]
    fn parses_and_deduplicates_third_party_uid_snapshot() {
        let uids = parse_third_party_uid_snapshot(
            "package:a uid:10123\npackage:b uid:10124\npackage:c uid:10123\n",
        );
        assert_eq!(uids, BTreeSet::from([10123, 10124]));
    }

    #[test]
    fn package_snapshot_delta_is_uid_scoped() {
        let previous = BTreeSet::from([10001, 10002]);
        let next = BTreeSet::from([10002, 10003]);
        assert_eq!(
            snapshot_delta(&previous, &next, 7),
            vec![
                SignalEvent::PackageAdded {
                    uid: 10003,
                    third_party: true,
                    monotonic_ns: 7
                },
                SignalEvent::PackageRemoved {
                    uid: 10001,
                    replacing: false,
                    monotonic_ns: 7
                }
            ]
        );
    }
}
