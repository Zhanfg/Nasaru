// SPDX-License-Identifier: GPL-3.0-or-later

use std::fs;
use std::io;
use std::mem::{offset_of, size_of};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub const DEFAULT_MAX_PACKET: usize = 4096;
pub const DEFAULT_BACKLOG: i32 = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerIdentity {
    pub pid: u32,
    pub uid: u32,
    pub gid: u32,
    pub security_context: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct PeerPolicy {
    allowed_uids: Vec<u32>,
    allowed_security_contexts: Vec<Vec<u8>>,
    require_security_context: bool,
}

impl PeerPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn root_only() -> Self {
        Self::new().allow_uid(0)
    }

    pub fn allow_uid(mut self, uid: u32) -> Self {
        if !self.allowed_uids.contains(&uid) {
            self.allowed_uids.push(uid);
        }
        self
    }

    pub fn allow_security_context(mut self, context: impl Into<Vec<u8>>) -> Self {
        let context = context.into();
        if !self.allowed_security_contexts.contains(&context) {
            self.allowed_security_contexts.push(context);
        }
        self
    }

    pub fn require_security_context(mut self, required: bool) -> Self {
        self.require_security_context = required;
        self
    }

    pub fn allows(&self, peer: &PeerIdentity) -> bool {
        if !self.allowed_uids.contains(&peer.uid) {
            return false;
        }

        if self.require_security_context && peer.security_context.is_none() {
            return false;
        }

        if self.allowed_security_contexts.is_empty() {
            return true;
        }

        peer.security_context.as_ref().is_some_and(|context| {
            self.allowed_security_contexts
                .iter()
                .any(|allowed| allowed == context)
        })
    }
}

#[derive(Debug)]
pub enum TransportError {
    Io(io::Error),
    PathTooLong,
    ExistingPathNotSocket,
    ExistingSocketWrongOwner { owner: u32, expected: u32 },
    PeerRejected(PeerIdentity),
    PacketTooLarge { max: usize },
    PartialSend,
}

impl From<io::Error> for TransportError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug)]
pub struct SeqPacketListener {
    fd: OwnedFd,
    path: PathBuf,
    socket_dev: u64,
    socket_ino: u64,
}

impl SeqPacketListener {
    pub fn bind(path: impl AsRef<Path>) -> Result<Self, TransportError> {
        Self::bind_with_backlog(path, DEFAULT_BACKLOG)
    }

    pub fn bind_with_backlog(
        path: impl AsRef<Path>,
        backlog: i32,
    ) -> Result<Self, TransportError> {
        let path = path.as_ref();
        prepare_socket_path(path)?;
        let address = unix_address(path)?;

        let raw_fd = unsafe {
            libc::socket(
                libc::AF_UNIX,
                libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
                0,
            )
        };
        if raw_fd < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };

        let bind_result = unsafe {
            libc::bind(
                fd.as_raw_fd(),
                &address.addr as *const libc::sockaddr_un as *const libc::sockaddr,
                address.len,
            )
        };
        if bind_result != 0 {
            return Err(io::Error::last_os_error().into());
        }

        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;

        if unsafe { libc::listen(fd.as_raw_fd(), backlog) } != 0 {
            return Err(io::Error::last_os_error().into());
        }

        let metadata = fs::symlink_metadata(path)?;
        Ok(Self {
            fd,
            path: path.to_path_buf(),
            socket_dev: metadata.dev(),
            socket_ino: metadata.ino(),
        })
    }

    pub fn accept(&self, policy: &PeerPolicy) -> Result<SeqPacketConnection, TransportError> {
        let raw_fd = unsafe {
            libc::accept4(
                self.fd.as_raw_fd(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                libc::SOCK_CLOEXEC,
            )
        };
        if raw_fd < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
        let peer = peer_identity(fd.as_raw_fd())?;

        if !policy.allows(&peer) {
            return Err(TransportError::PeerRejected(peer));
        }

        Ok(SeqPacketConnection { fd, peer })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for SeqPacketListener {
    fn drop(&mut self) {
        let Ok(metadata) = fs::symlink_metadata(&self.path) else {
            return;
        };
        if metadata.file_type().is_socket()
            && metadata.dev() == self.socket_dev
            && metadata.ino() == self.socket_ino
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[derive(Debug)]
pub struct SeqPacketConnection {
    fd: OwnedFd,
    peer: PeerIdentity,
}

impl SeqPacketConnection {
    pub fn connect(path: impl AsRef<Path>) -> Result<Self, TransportError> {
        let address = unix_address(path.as_ref())?;
        let raw_fd = unsafe {
            libc::socket(
                libc::AF_UNIX,
                libc::SOCK_SEQPACKET | libc::SOCK_CLOEXEC,
                0,
            )
        };
        if raw_fd < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };

        let result = unsafe {
            libc::connect(
                fd.as_raw_fd(),
                &address.addr as *const libc::sockaddr_un as *const libc::sockaddr,
                address.len,
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error().into());
        }

        let peer = peer_identity(fd.as_raw_fd())?;
        Ok(Self { fd, peer })
    }

    pub fn peer(&self) -> &PeerIdentity {
        &self.peer
    }

    pub fn send_packet(&self, packet: &[u8]) -> Result<(), TransportError> {
        let sent = unsafe {
            libc::send(
                self.fd.as_raw_fd(),
                packet.as_ptr().cast(),
                packet.len(),
                libc::MSG_NOSIGNAL,
            )
        };
        if sent < 0 {
            return Err(io::Error::last_os_error().into());
        }
        if sent as usize != packet.len() {
            return Err(TransportError::PartialSend);
        }
        Ok(())
    }

    pub fn recv_packet(&self, max: usize) -> Result<Option<Vec<u8>>, TransportError> {
        if max == 0 {
            return Err(TransportError::PacketTooLarge { max });
        }

        let mut buffer = vec![0u8; max.saturating_add(1)];
        let received = unsafe {
            libc::recv(
                self.fd.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                0,
            )
        };
        if received < 0 {
            return Err(io::Error::last_os_error().into());
        }
        if received == 0 {
            return Ok(None);
        }

        let received = received as usize;
        if received > max {
            return Err(TransportError::PacketTooLarge { max });
        }

        buffer.truncate(received);
        Ok(Some(buffer))
    }
}

struct UnixAddress {
    addr: libc::sockaddr_un,
    len: libc::socklen_t,
}

fn unix_address(path: &Path) -> Result<UnixAddress, TransportError> {
    let bytes = path.as_os_str().as_bytes();
    let mut addr: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.len() + 1 > addr.sun_path.len() {
        return Err(TransportError::PathTooLong);
    }

    addr.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (dst, src) in addr.sun_path.iter_mut().zip(bytes.iter().copied()) {
        *dst = src as libc::c_char;
    }

    let len = offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1;
    Ok(UnixAddress {
        addr,
        len: len as libc::socklen_t,
    })
}

fn prepare_socket_path(path: &Path) -> Result<(), TransportError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }

    let Ok(metadata) = fs::symlink_metadata(path) else {
        return Ok(());
    };

    if !metadata.file_type().is_socket() {
        return Err(TransportError::ExistingPathNotSocket);
    }

    let expected = unsafe { libc::geteuid() };
    if metadata.uid() != expected {
        return Err(TransportError::ExistingSocketWrongOwner {
            owner: metadata.uid(),
            expected,
        });
    }

    fs::remove_file(path)?;
    Ok(())
}

fn peer_identity(fd: RawFd) -> Result<PeerIdentity, TransportError> {
    let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
    let mut credentials_len = size_of::<libc::ucred>() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut credentials as *mut libc::ucred).cast(),
            &mut credentials_len,
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error().into());
    }

    Ok(PeerIdentity {
        pid: credentials.pid as u32,
        uid: credentials.uid,
        gid: credentials.gid,
        security_context: peer_security_context(fd)?,
    })
}

fn peer_security_context(fd: RawFd) -> Result<Option<Vec<u8>>, TransportError> {
    let mut buffer = vec![0u8; 4096];
    let mut len = buffer.len() as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERSEC,
            buffer.as_mut_ptr().cast(),
            &mut len,
        )
    };
    if result != 0 {
        let error = io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(libc::ENOPROTOOPT) | Some(libc::EINVAL) => Ok(None),
            _ => Err(error.into()),
        };
    }

    buffer.truncate(len as usize);
    while buffer.last() == Some(&0) {
        buffer.pop();
    }
    Ok(Some(buffer))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::thread;

    static NEXT_PATH: AtomicU64 = AtomicU64::new(1);

    fn temp_socket(name: &str) -> PathBuf {
        let sequence = NEXT_PATH.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "nasaru-{name}-{}-{sequence}.sock",
            std::process::id()
        ))
    }

    #[test]
    fn policy_requires_explicit_uid() {
        let peer = PeerIdentity {
            pid: 1,
            uid: 123,
            gid: 123,
            security_context: None,
        };
        assert!(!PeerPolicy::new().allows(&peer));
        assert!(PeerPolicy::new().allow_uid(123).allows(&peer));
    }

    #[test]
    fn policy_can_require_specific_security_context() {
        let peer = PeerIdentity {
            pid: 1,
            uid: 123,
            gid: 123,
            security_context: Some(b"u:r:nasaru_collector:s0".to_vec()),
        };
        let policy = PeerPolicy::new()
            .allow_uid(123)
            .allow_security_context(b"u:r:nasaru_collector:s0".to_vec())
            .require_security_context(true);
        assert!(policy.allows(&peer));
    }

    #[test]
    fn seqpacket_preserves_one_riksu_frame_per_receive() {
        let path = temp_socket("roundtrip");
        let listener = SeqPacketListener::bind(&path).unwrap();
        let uid = unsafe { libc::geteuid() };

        let server = thread::spawn(move || {
            let connection = listener
                .accept(&PeerPolicy::new().allow_uid(uid))
                .unwrap();
            let packet = connection.recv_packet(128).unwrap().unwrap();
            assert_eq!(packet, b"riksu-event");
            connection.send_packet(b"ack").unwrap();
        });

        let client = SeqPacketConnection::connect(&path).unwrap();
        client.send_packet(b"riksu-event").unwrap();
        assert_eq!(client.recv_packet(128).unwrap().unwrap(), b"ack");
        server.join().unwrap();
    }

    #[test]
    fn existing_regular_file_is_never_unlinked() {
        let path = temp_socket("regular");
        fs::write(&path, b"do-not-delete").unwrap();
        assert!(matches!(
            SeqPacketListener::bind(&path),
            Err(TransportError::ExistingPathNotSocket)
        ));
        assert_eq!(fs::read(&path).unwrap(), b"do-not-delete");
        fs::remove_file(path).unwrap();
    }
}
