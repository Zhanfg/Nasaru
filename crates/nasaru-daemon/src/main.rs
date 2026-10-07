// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_daemon::NasaruDaemon;
use nasaru_local_transport::{PeerPolicy, SeqPacketListener};
use std::path::PathBuf;

const DEFAULT_SOCKET_PATH: &str = "/data/adb/nasaru/run/collector.sock";

fn main() {
    let socket = std::env::var_os("NASARU_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET_PATH));

    let listener = match SeqPacketListener::bind(&socket) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("nasarud: failed to bind {}: {error:?}", socket.display());
            std::process::exit(1);
        }
    };

    let policy = PeerPolicy::root_only();
    let mut daemon = NasaruDaemon::default();

    loop {
        match daemon.serve_one_collector(&listener, &policy) {
            Ok(processed) => {
                eprintln!("nasarud: collector disconnected after {processed} events");
            }
            Err(error) => {
                eprintln!("nasarud: collector session failed: {error:?}");
            }
        }
    }
}
