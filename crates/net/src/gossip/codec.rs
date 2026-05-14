//! Borsh codec for gossip payloads.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::error::{Error, Result};

/// Encode a Borsh-serializable value as gossip payload bytes.
pub fn encode_action_payload<T: BorshSerialize>(value: &T) -> Result<Vec<u8>> {
    borsh::to_vec(value).map_err(|e| Error::Codec(e.to_string()))
}

/// Decode a Borsh-serializable value from gossip payload bytes.
pub fn decode_event_payload<T: BorshDeserialize>(bytes: &[u8]) -> Result<T> {
    borsh::from_slice(bytes).map_err(|e| Error::Codec(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use consensus::event::TimerId;

    #[test]
    fn round_trip_via_payload() {
        let t = TimerId(42);
        let bytes = encode_action_payload(&t).unwrap();
        let t2: TimerId = decode_event_payload(&bytes).unwrap();
        assert_eq!(t, t2);
    }
}
