// SPDX-License-Identifier: GPL-3.0-or-later

use nasaru_riksu::{
    decode_tlvs, MessageType, RiksuError, RiksuHeader, Tlv, RIKSU_HEADER_LEN, RIKSU_MAJOR,
    RIKSU_MINOR,
};
use nasaru_signal_state::{ActiveOp, FgsTypes, SignalEvent, UidImportance};

const TLV_EVENT_KIND: u16 = 1;
const TLV_UID: u16 = 2;
const TLV_IMPORTANCE: u16 = 3;
const TLV_FGS_TYPES: u16 = 4;
const TLV_ACTIVE_OP: u16 = 5;
const TLV_ACTIVE: u16 = 6;
const TLV_PRESENT: u16 = 7;
const TLV_THIRD_PARTY: u16 = 8;
const TLV_REPLACING: u16 = 9;

const EVENT_UID_IMPORTANCE: u8 = 1;
const EVENT_FGS_TYPES: u8 = 2;
const EVENT_APPOP_ACTIVE: u8 = 3;
const EVENT_COMPANION_PRESENCE: u8 = 4;
const EVENT_PACKAGE_ADDED: u8 = 5;
const EVENT_PACKAGE_REMOVED: u8 = 6;

pub const MAX_SIGNAL_PAYLOAD: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireError {
    Riksu(RiksuError),
    UnsupportedMajor(u8),
    WrongMessageType,
    LengthMismatch,
    PayloadTooLarge,
    MissingField(u16),
    DuplicateField(u16),
    InvalidField(u16),
}

impl From<RiksuError> for WireError {
    fn from(value: RiksuError) -> Self {
        Self::Riksu(value)
    }
}

pub fn encode_signal_event(sequence: u64, event: SignalEvent) -> Result<Vec<u8>, WireError> {
    let mut tlvs = Vec::new();
    push_u8(&mut tlvs, TLV_EVENT_KIND, event_kind(event));
    push_u32(&mut tlvs, TLV_UID, event.uid());

    match event {
        SignalEvent::UidImportanceChanged { importance, .. } => {
            push_u8(&mut tlvs, TLV_IMPORTANCE, importance_to_wire(importance));
        }
        SignalEvent::ForegroundServiceTypesChanged { types, .. } => {
            push_u16(&mut tlvs, TLV_FGS_TYPES, types.bits());
        }
        SignalEvent::AppOpActiveChanged { op, active, .. } => {
            push_u8(&mut tlvs, TLV_ACTIVE_OP, active_op_to_wire(op));
            push_bool(&mut tlvs, TLV_ACTIVE, active);
        }
        SignalEvent::CompanionPresenceChanged { present, .. } => {
            push_bool(&mut tlvs, TLV_PRESENT, present);
        }
        SignalEvent::PackageAdded { third_party, .. } => {
            push_bool(&mut tlvs, TLV_THIRD_PARTY, third_party);
        }
        SignalEvent::PackageRemoved { replacing, .. } => {
            push_bool(&mut tlvs, TLV_REPLACING, replacing);
        }
    }

    let mut payload = Vec::new();
    for tlv in tlvs {
        tlv.encode(&mut payload)?;
    }
    if payload.len() > MAX_SIGNAL_PAYLOAD {
        return Err(WireError::PayloadTooLarge);
    }

    let header = RiksuHeader {
        major: RIKSU_MAJOR,
        minor: RIKSU_MINOR,
        message_type: MessageType::Event,
        flags: 0,
        payload_len: payload.len() as u32,
        sequence,
        monotonic_ns: event.monotonic_ns(),
    };

    let mut frame = Vec::with_capacity(RIKSU_HEADER_LEN + payload.len());
    frame.extend_from_slice(&header.encode());
    frame.extend_from_slice(&payload);
    Ok(frame)
}

pub fn decode_signal_event(frame: &[u8]) -> Result<SignalEvent, WireError> {
    let header = RiksuHeader::decode(frame)?;
    if header.major != RIKSU_MAJOR {
        return Err(WireError::UnsupportedMajor(header.major));
    }
    if header.message_type != MessageType::Event {
        return Err(WireError::WrongMessageType);
    }

    let payload_len = header.payload_len as usize;
    if payload_len > MAX_SIGNAL_PAYLOAD {
        return Err(WireError::PayloadTooLarge);
    }
    if frame.len() != RIKSU_HEADER_LEN + payload_len {
        return Err(WireError::LengthMismatch);
    }

    let tlvs = decode_tlvs(&frame[RIKSU_HEADER_LEN..])?;
    let kind = one_u8(&tlvs, TLV_EVENT_KIND)?;
    let uid = one_u32(&tlvs, TLV_UID)?;
    let monotonic_ns = header.monotonic_ns;

    match kind {
        EVENT_UID_IMPORTANCE => Ok(SignalEvent::UidImportanceChanged {
            uid,
            importance: importance_from_wire(one_u8(&tlvs, TLV_IMPORTANCE)?)?,
            monotonic_ns,
        }),
        EVENT_FGS_TYPES => Ok(SignalEvent::ForegroundServiceTypesChanged {
            uid,
            types: FgsTypes::from_bits(one_u16(&tlvs, TLV_FGS_TYPES)?),
            monotonic_ns,
        }),
        EVENT_APPOP_ACTIVE => Ok(SignalEvent::AppOpActiveChanged {
            uid,
            op: active_op_from_wire(one_u8(&tlvs, TLV_ACTIVE_OP)?)?,
            active: one_bool(&tlvs, TLV_ACTIVE)?,
            monotonic_ns,
        }),
        EVENT_COMPANION_PRESENCE => Ok(SignalEvent::CompanionPresenceChanged {
            uid,
            present: one_bool(&tlvs, TLV_PRESENT)?,
            monotonic_ns,
        }),
        EVENT_PACKAGE_ADDED => Ok(SignalEvent::PackageAdded {
            uid,
            third_party: one_bool(&tlvs, TLV_THIRD_PARTY)?,
            monotonic_ns,
        }),
        EVENT_PACKAGE_REMOVED => Ok(SignalEvent::PackageRemoved {
            uid,
            replacing: one_bool(&tlvs, TLV_REPLACING)?,
            monotonic_ns,
        }),
        _ => Err(WireError::InvalidField(TLV_EVENT_KIND)),
    }
}

fn event_kind(event: SignalEvent) -> u8 {
    match event {
        SignalEvent::UidImportanceChanged { .. } => EVENT_UID_IMPORTANCE,
        SignalEvent::ForegroundServiceTypesChanged { .. } => EVENT_FGS_TYPES,
        SignalEvent::AppOpActiveChanged { .. } => EVENT_APPOP_ACTIVE,
        SignalEvent::CompanionPresenceChanged { .. } => EVENT_COMPANION_PRESENCE,
        SignalEvent::PackageAdded { .. } => EVENT_PACKAGE_ADDED,
        SignalEvent::PackageRemoved { .. } => EVENT_PACKAGE_REMOVED,
    }
}

fn importance_to_wire(value: UidImportance) -> u8 {
    match value {
        UidImportance::Foreground => 1,
        UidImportance::ForegroundService => 2,
        UidImportance::Visible => 3,
        UidImportance::Perceptible => 4,
        UidImportance::Service => 5,
        UidImportance::Cached => 6,
        UidImportance::Gone => 7,
    }
}

fn importance_from_wire(value: u8) -> Result<UidImportance, WireError> {
    match value {
        1 => Ok(UidImportance::Foreground),
        2 => Ok(UidImportance::ForegroundService),
        3 => Ok(UidImportance::Visible),
        4 => Ok(UidImportance::Perceptible),
        5 => Ok(UidImportance::Service),
        6 => Ok(UidImportance::Cached),
        7 => Ok(UidImportance::Gone),
        _ => Err(WireError::InvalidField(TLV_IMPORTANCE)),
    }
}

fn active_op_to_wire(value: ActiveOp) -> u8 {
    match value {
        ActiveOp::Camera => 1,
        ActiveOp::Microphone => 2,
        ActiveOp::Location => 3,
    }
}

fn active_op_from_wire(value: u8) -> Result<ActiveOp, WireError> {
    match value {
        1 => Ok(ActiveOp::Camera),
        2 => Ok(ActiveOp::Microphone),
        3 => Ok(ActiveOp::Location),
        _ => Err(WireError::InvalidField(TLV_ACTIVE_OP)),
    }
}

fn push_u8(out: &mut Vec<Tlv>, kind: u16, value: u8) {
    out.push(Tlv {
        kind,
        value: vec![value],
    });
}

fn push_bool(out: &mut Vec<Tlv>, kind: u16, value: bool) {
    push_u8(out, kind, u8::from(value));
}

fn push_u16(out: &mut Vec<Tlv>, kind: u16, value: u16) {
    out.push(Tlv {
        kind,
        value: value.to_le_bytes().to_vec(),
    });
}

fn push_u32(out: &mut Vec<Tlv>, kind: u16, value: u32) {
    out.push(Tlv {
        kind,
        value: value.to_le_bytes().to_vec(),
    });
}

fn one(tlvs: &[Tlv], kind: u16) -> Result<&[u8], WireError> {
    let mut matches = tlvs.iter().filter(|tlv| tlv.kind == kind);
    let value = matches
        .next()
        .ok_or(WireError::MissingField(kind))?
        .value
        .as_slice();
    if matches.next().is_some() {
        return Err(WireError::DuplicateField(kind));
    }
    Ok(value)
}

fn one_u8(tlvs: &[Tlv], kind: u16) -> Result<u8, WireError> {
    let value = one(tlvs, kind)?;
    if value.len() != 1 {
        return Err(WireError::InvalidField(kind));
    }
    Ok(value[0])
}

fn one_bool(tlvs: &[Tlv], kind: u16) -> Result<bool, WireError> {
    match one_u8(tlvs, kind)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(WireError::InvalidField(kind)),
    }
}

fn one_u16(tlvs: &[Tlv], kind: u16) -> Result<u16, WireError> {
    let value = one(tlvs, kind)?;
    let bytes: [u8; 2] = value
        .try_into()
        .map_err(|_| WireError::InvalidField(kind))?;
    Ok(u16::from_le_bytes(bytes))
}

fn one_u32(tlvs: &[Tlv], kind: u16) -> Result<u32, WireError> {
    let value = one(tlvs, kind)?;
    let bytes: [u8; 4] = value
        .try_into()
        .map_err(|_| WireError::InvalidField(kind))?;
    Ok(u32::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cases() -> Vec<SignalEvent> {
        vec![
            SignalEvent::UidImportanceChanged {
                uid: 10001,
                importance: UidImportance::ForegroundService,
                monotonic_ns: 11,
            },
            SignalEvent::ForegroundServiceTypesChanged {
                uid: 10002,
                types: FgsTypes::LOCATION.union(FgsTypes::HEALTH),
                monotonic_ns: 12,
            },
            SignalEvent::AppOpActiveChanged {
                uid: 10003,
                op: ActiveOp::Microphone,
                active: true,
                monotonic_ns: 13,
            },
            SignalEvent::CompanionPresenceChanged {
                uid: 10004,
                present: true,
                monotonic_ns: 14,
            },
            SignalEvent::PackageAdded {
                uid: 10005,
                third_party: true,
                monotonic_ns: 15,
            },
            SignalEvent::PackageRemoved {
                uid: 10006,
                replacing: false,
                monotonic_ns: 16,
            },
        ]
    }

    #[test]
    fn package_added_golden_frame_is_stable() {
        let event = SignalEvent::PackageAdded {
            uid: 42,
            third_party: true,
            monotonic_ns: 9,
        };
        let frame = encode_signal_event(7, event).unwrap();
        let expected: [u8; 50] = [
            0x52, 0x4b, 0x53, 0x55, 0x01, 0x00, 0x10, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x12, 0x00, 0x00, 0x00,
            0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x01, 0x00, 0x05,
            0x02, 0x00, 0x04, 0x00, 0x2a, 0x00, 0x00, 0x00,
            0x08, 0x00, 0x01, 0x00, 0x01,
        ];
        assert_eq!(frame, expected);
    }

    #[test]
    fn all_signal_events_roundtrip() {
        for (sequence, event) in cases().into_iter().enumerate() {
            let frame = encode_signal_event(sequence as u64, event).unwrap();
            assert_eq!(decode_signal_event(&frame).unwrap(), event);
        }
    }

    #[test]
    fn unknown_tlv_is_forward_compatible() {
        let event = SignalEvent::PackageAdded {
            uid: 42,
            third_party: true,
            monotonic_ns: 9,
        };
        let frame = encode_signal_event(1, event).unwrap();
        let header = RiksuHeader::decode(&frame).unwrap();
        let mut payload = frame[RIKSU_HEADER_LEN..].to_vec();
        Tlv {
            kind: 65000,
            value: vec![1, 2, 3],
        }
        .encode(&mut payload)
        .unwrap();
        let mut new_header = header;
        new_header.payload_len = payload.len() as u32;
        let mut extended = new_header.encode().to_vec();
        extended.extend_from_slice(&payload);
        assert_eq!(decode_signal_event(&extended).unwrap(), event);
    }

    #[test]
    fn duplicate_security_field_is_rejected() {
        let event = SignalEvent::PackageAdded {
            uid: 42,
            third_party: true,
            monotonic_ns: 9,
        };
        let frame = encode_signal_event(1, event).unwrap();
        let header = RiksuHeader::decode(&frame).unwrap();
        let mut payload = frame[RIKSU_HEADER_LEN..].to_vec();
        Tlv {
            kind: TLV_UID,
            value: 99u32.to_le_bytes().to_vec(),
        }
        .encode(&mut payload)
        .unwrap();
        let mut new_header = header;
        new_header.payload_len = payload.len() as u32;
        let mut duplicated = new_header.encode().to_vec();
        duplicated.extend_from_slice(&payload);
        assert_eq!(
            decode_signal_event(&duplicated),
            Err(WireError::DuplicateField(TLV_UID))
        );
    }

    #[test]
    fn payload_length_mismatch_is_rejected() {
        let event = SignalEvent::CompanionPresenceChanged {
            uid: 7,
            present: false,
            monotonic_ns: 9,
        };
        let mut frame = encode_signal_event(1, event).unwrap();
        frame.push(0);
        assert_eq!(decode_signal_event(&frame), Err(WireError::LengthMismatch));
    }
}
