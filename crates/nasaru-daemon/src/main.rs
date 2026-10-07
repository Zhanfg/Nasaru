// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_daemon::NasaruDaemon;

fn main() {
    let daemon = NasaruDaemon::default();
    println!(
        "nasarud M1 runtime ready; next_expiry_ns={:?}",
        daemon.runtime().next_expiry_ns()
    );
}
