// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_policy::{decide, Context, Resource};
use nasaru_riksu::{MessageType, RiksuHeader, RIKSU_MAJOR, RIKSU_MINOR};

fn main() {
    let probe = decide(Resource::Camera, Context::default());
    let hello = RiksuHeader {
        major: RIKSU_MAJOR,
        minor: RIKSU_MINOR,
        message_type: MessageType::Hello,
        flags: 0,
        payload_len: 0,
        sequence: 1,
        monotonic_ns: 0,
    };

    println!(
        "nasarud bootstrap: policy={probe:?}, riksu_header_bytes={}",
        hello.encode().len()
    );
}
