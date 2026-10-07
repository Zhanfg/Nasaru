// SPDX-License-Identifier: GPL-3.0-or-later

pub const RIKSU_MAGIC: [u8; 4] = *b"RKSU";
pub const RIKSU_HEADER_LEN: usize = 32;
pub const RIKSU_MAJOR: u8 = 1;
pub const RIKSU_MINOR: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum MessageType {
    Hello = 0x0001,
    Status = 0x0002,
    Event = 0x0010,
    Decision = 0x0011,
    QibituGrant = 0x0020,
    QibituRevoke = 0x0021,
    QibituExpire = 0x0022,
    PolicySnapshot = 0x0030,
    PolicyDelta = 0x0031,
    Feedback = 0x0040,
    ModelInfo = 0x0050,
    AuditEvent = 0x0060,
}

impl TryFrom<u16> for MessageType {
    type Error = RiksuError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        use MessageType::*;
        Ok(match value {
            0x0001 => Hello,
            0x0002 => Status,
            0x0010 => Event,
            0x0011 => Decision,
            0x0020 => QibituGrant,
            0x0021 => QibituRevoke,
            0x0022 => QibituExpire,
            0x0030 => PolicySnapshot,
            0x0031 => PolicyDelta,
            0x0040 => Feedback,
            0x0050 => ModelInfo,
            0x0060 => AuditEvent,
            other => return Err(RiksuError::UnknownMessageType(other)),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiksuHeader {
    pub major: u8,
    pub minor: u8,
    pub message_type: MessageType,
    pub flags: u32,
    pub payload_len: u32,
    pub sequence: u64,
    pub monotonic_ns: u64,
}

impl RiksuHeader {
    pub fn encode(self) -> [u8; RIKSU_HEADER_LEN] {
        let mut out = [0u8; RIKSU_HEADER_LEN];
        out[0..4].copy_from_slice(&RIKSU_MAGIC);
        out[4] = self.major;
        out[5] = self.minor;
        out[6..8].copy_from_slice(&(self.message_type as u16).to_le_bytes());
        out[8..12].copy_from_slice(&self.flags.to_le_bytes());
        out[12..16].copy_from_slice(&self.payload_len.to_le_bytes());
        out[16..24].copy_from_slice(&self.sequence.to_le_bytes());
        out[24..32].copy_from_slice(&self.monotonic_ns.to_le_bytes());
        out
    }

    pub fn decode(input: &[u8]) -> Result<Self, RiksuError> {
        if input.len() < RIKSU_HEADER_LEN {
            return Err(RiksuError::Truncated);
        }
        if input[0..4] != RIKSU_MAGIC {
            return Err(RiksuError::BadMagic);
        }
        Ok(Self {
            major: input[4],
            minor: input[5],
            message_type: u16::from_le_bytes([input[6], input[7]]).try_into()?,
            flags: u32::from_le_bytes(input[8..12].try_into().unwrap()),
            payload_len: u32::from_le_bytes(input[12..16].try_into().unwrap()),
            sequence: u64::from_le_bytes(input[16..24].try_into().unwrap()),
            monotonic_ns: u64::from_le_bytes(input[24..32].try_into().unwrap()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tlv {
    pub kind: u16,
    pub value: Vec<u8>,
}

impl Tlv {
    pub fn encode(&self, out: &mut Vec<u8>) -> Result<(), RiksuError> {
        let len = u16::try_from(self.value.len()).map_err(|_| RiksuError::ValueTooLong)?;
        out.extend_from_slice(&self.kind.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&self.value);
        Ok(())
    }
}

pub fn decode_tlvs(mut input: &[u8]) -> Result<Vec<Tlv>, RiksuError> {
    let mut out = Vec::new();
    while !input.is_empty() {
        if input.len() < 4 {
            return Err(RiksuError::Truncated);
        }
        let kind = u16::from_le_bytes([input[0], input[1]]);
        let len = u16::from_le_bytes([input[2], input[3]]) as usize;
        input = &input[4..];
        if input.len() < len {
            return Err(RiksuError::Truncated);
        }
        out.push(Tlv { kind, value: input[..len].to_vec() });
        input = &input[len..];
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum QibituAction {
    Allow = 1,
    Deny = 2,
    Redact = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Qibitu {
    pub uid: u32,
    pub resource: u16,
    pub action: QibituAction,
    pub scope: u16,
    pub issued_monotonic_ns: u64,
    pub ttl_ms: u32,
}

impl Qibitu {
    pub fn is_expired(&self, now_monotonic_ns: u64) -> bool {
        let ttl_ns = u64::from(self.ttl_ms).saturating_mul(1_000_000);
        now_monotonic_ns.saturating_sub(self.issued_monotonic_ns) >= ttl_ns
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiksuError {
    Truncated,
    BadMagic,
    UnknownMessageType(u16),
    ValueTooLong,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip_is_exactly_32_bytes() {
        let header = RiksuHeader {
            major: RIKSU_MAJOR,
            minor: RIKSU_MINOR,
            message_type: MessageType::Decision,
            flags: 3,
            payload_len: 7,
            sequence: 42,
            monotonic_ns: 99,
        };
        let encoded = header.encode();
        assert_eq!(encoded.len(), 32);
        assert_eq!(RiksuHeader::decode(&encoded).unwrap(), header);
    }

    #[test]
    fn tlv_unknown_fields_remain_skippable() {
        let source = vec![
            Tlv { kind: 1, value: vec![1, 2, 3, 4] },
            Tlv { kind: 65000, value: vec![9, 8, 7] },
        ];
        let mut wire = Vec::new();
        for item in &source {
            item.encode(&mut wire).unwrap();
        }
        assert_eq!(decode_tlvs(&wire).unwrap(), source);
    }

    #[test]
    fn qibitu_expires_using_monotonic_time() {
        let lease = Qibitu {
            uid: 10001,
            resource: 3,
            action: QibituAction::Allow,
            scope: 7,
            issued_monotonic_ns: 1_000_000_000,
            ttl_ms: 500,
        };
        assert!(!lease.is_expired(1_499_999_999));
        assert!(lease.is_expired(1_500_000_000));
    }
}
