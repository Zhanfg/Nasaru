// SPDX-License-Identifier: GPL-3.0-or-later

pub const MICRO_MAGIC: [u8; 4] = *b"NSM1";
pub const MICRO_HEADER_LEN: usize = 12;
pub const MICRO_Q_SHIFT: u8 = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicroModel {
    pub schema_version: u16,
    pub weights_q12: Vec<i16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroError {
    Truncated,
    BadMagic,
    BadLength,
    FeatureOutOfRange,
}

impl MicroModel {
    pub fn parse(input: &[u8]) -> Result<Self, MicroError> {
        if input.len() < MICRO_HEADER_LEN {
            return Err(MicroError::Truncated);
        }
        if input[0..4] != MICRO_MAGIC {
            return Err(MicroError::BadMagic);
        }
        let schema_version = u16::from_le_bytes([input[4], input[5]]);
        let dims = u16::from_le_bytes([input[6], input[7]]) as usize;
        if input[8] != MICRO_Q_SHIFT {
            return Err(MicroError::BadLength);
        }
        let expected = MICRO_HEADER_LEN + dims * 2;
        if input.len() != expected {
            return Err(MicroError::BadLength);
        }
        let mut weights_q12 = Vec::with_capacity(dims);
        let (pairs, remainder) = input[MICRO_HEADER_LEN..].as_chunks::<2>();
        if !remainder.is_empty() {
            return Err(MicroError::BadLength);
        }
        for pair in pairs {
            weights_q12.push(i16::from_le_bytes(*pair));
        }
        Ok(Self {
            schema_version,
            weights_q12,
        })
    }

    pub fn score_sparse(&self, active_features: &[usize]) -> Result<i32, MicroError> {
        if self.weights_q12.is_empty() {
            return Err(MicroError::BadLength);
        }
        let mut score = i32::from(self.weights_q12[0]);
        for &index in active_features {
            if index == 0 || index >= self.weights_q12.len() {
                return Err(MicroError::FeatureOutOfRange);
            }
            score = score.saturating_add(i32::from(self.weights_q12[index]));
        }
        Ok(score)
    }

    pub fn predicts_legitimate(&self, active_features: &[usize]) -> Result<bool, MicroError> {
        Ok(self.score_sparse(active_features)? >= 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_bytes() -> Vec<u8> {
        let weights = [-2048i16, 4096, -4096];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MICRO_MAGIC);
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&(weights.len() as u16).to_le_bytes());
        bytes.push(MICRO_Q_SHIFT);
        bytes.extend_from_slice(&[0, 0, 0]);
        for value in weights {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn integer_model_roundtrip() {
        let model = MicroModel::parse(&model_bytes()).unwrap();
        assert!(model.predicts_legitimate(&[1]).unwrap());
        assert!(!model.predicts_legitimate(&[2]).unwrap());
    }
}
