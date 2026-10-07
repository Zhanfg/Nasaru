// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_android_command_collector::{
    run_native_collector, CommandPaths, DEFAULT_SOCKET_PATH,
};

fn main() {
    let socket = std::env::var_os("NASARU_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_SOCKET_PATH));

    if let Err(error) = run_native_collector(socket, CommandPaths::default()) {
        eprintln!("nasaru-collector stopped: {error:?}");
        std::process::exit(1);
    }
}
