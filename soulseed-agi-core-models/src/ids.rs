use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
    sync::{Mutex, OnceLock},
};

#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const EPOCH_UNIX_MS: i64 = 1_704_064_000_000; // 2024-01-01T00:00:00Z
const TIMESTAMP_BITS: u64 = 42;
const SHARD_BITS: u64 = 10;
const SEQUENCE_BITS: u64 = 12;
const MAX_TIMESTAMP: u64 = (1 << TIMESTAMP_BITS) - 1;
const MAX_SHARD_ID: u16 = (1 << SHARD_BITS) - 1;
const MAX_SEQUENCE: u16 = (1 << SEQUENCE_BITS) - 1;
const SHARD_SHIFT: u64 = SEQUENCE_BITS;
const TIMESTAMP_SHIFT: u64 = SHARD_BITS + SEQUENCE_BITS;
const BASE36_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdError {
    #[error("global id factory has already been configured")]
    AlreadyConfigured,
    #[error("shard id {0} exceeds maximum {MAX_SHARD_ID}")]
    ShardOutOfRange(u16),
    #[error("clock moved backwards: drift {0} ms exceeds allowed threshold")]
    ClockWentBackwards(u64),
    #[error("timestamp overflow: {0} >= {MAX_TIMESTAMP}")]
    TimestampOverflow(u64),
    #[error("sequence overflow for timestamp {timestamp_ms}")]
    SequenceOverflow { timestamp_ms: u64 },
    #[error("base36 parse error")]
    InvalidBase36,
    #[error("raw id out of range")]
    RawOutOfRange,
}

pub type IdResult<T> = Result<T, IdError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IdComponents {
    pub timestamp_ms: u64,
    pub shard_id: u16,
    pub sequence: u16,
}

impl IdComponents {
    pub fn unix_epoch_ms(&self) -> u64 {
        EPOCH_UNIX_MS
            .saturating_add(self.timestamp_ms as i64)
            .try_into()
            .unwrap_or(0)
    }
}

#[derive(Debug)]
struct FactoryState {
    last_timestamp: u64,
    sequence: u16,
}

pub struct GlobalIdFactory {
    shard_id: u16,
    state: Mutex<FactoryState>,
}

impl GlobalIdFactory {
    fn new(shard_id: u16) -> Self {
        GlobalIdFactory {
            shard_id,
            state: Mutex::new(FactoryState {
                last_timestamp: 0,
                sequence: 0,
            }),
        }
    }

    fn now_millis(&self) -> IdResult<u64> {
        #[cfg(not(target_arch = "wasm32"))]
        let millis = {
            let now = SystemTime::now();
            let since_epoch = now
                .duration_since(UNIX_EPOCH)
                .unwrap_or_else(|_| Duration::from_millis(0));
            since_epoch.as_millis() as i64
        };

        #[cfg(target_arch = "wasm32")]
        let millis = js_sys::Date::now() as i64;

        let adjusted = millis - EPOCH_UNIX_MS;
        if adjusted < 0 {
            return Err(IdError::ClockWentBackwards(adjusted.unsigned_abs()));
        }
        let timestamp = adjusted as u64;
        if timestamp > MAX_TIMESTAMP {
            return Err(IdError::TimestampOverflow(timestamp));
        }
        Ok(timestamp)
    }

    fn wait_for_next_millis(&self, current: u64) -> IdResult<u64> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            loop {
                let ts = self.now_millis()?;
                if ts > current {
                    return Ok(ts);
                }
                std::thread::yield_now();
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            // In WASM, we can't block, so just try a few times
            for _ in 0..100 {
                let ts = self.now_millis()?;
                if ts > current {
                    return Ok(ts);
                }
            }
            // If still same millisecond, just return current + 1
            Ok(current + 1)
        }
    }

    pub fn next_id(&self) -> IdResult<u64> {
        let mut guard = self.state.lock().expect("mutex poisoned");
        let mut timestamp = self.now_millis()?;

        if timestamp < guard.last_timestamp {
            return Err(IdError::ClockWentBackwards(
                guard.last_timestamp - timestamp,
            ));
        }

        if timestamp == guard.last_timestamp {
            if guard.sequence == MAX_SEQUENCE {
                timestamp = self.wait_for_next_millis(guard.last_timestamp)?;
                guard.sequence = 0;
                guard.last_timestamp = timestamp;
            } else {
                guard.sequence += 1;
            }
        } else {
            guard.sequence = 0;
            guard.last_timestamp = timestamp;
        }

        Ok(compose_raw_id(timestamp, self.shard_id, guard.sequence))
    }

    pub fn validate_raw(raw: u64) -> IdResult<IdComponents> {
        let timestamp = raw >> TIMESTAMP_SHIFT;
        if timestamp > MAX_TIMESTAMP {
            return Err(IdError::TimestampOverflow(timestamp));
        }
        let shard_id = ((raw >> SHARD_SHIFT) & ((1 << SHARD_BITS) - 1)) as u16;
        if shard_id > MAX_SHARD_ID {
            return Err(IdError::ShardOutOfRange(shard_id));
        }
        let sequence = (raw & ((1 << SEQUENCE_BITS) - 1)) as u16;
        if sequence > MAX_SEQUENCE {
            return Err(IdError::SequenceOverflow {
                timestamp_ms: timestamp,
            });
        }

        Ok(IdComponents {
            timestamp_ms: timestamp,
            shard_id,
            sequence,
        })
    }

    pub fn configure_global(shard_id: u16) -> IdResult<()> {
        if shard_id > MAX_SHARD_ID {
            return Err(IdError::ShardOutOfRange(shard_id));
        }
        GLOBAL_FACTORY
            .set(GlobalIdFactory::new(shard_id))
            .map_err(|_| IdError::AlreadyConfigured)
    }
}

fn compose_raw_id(timestamp: u64, shard_id: u16, sequence: u16) -> u64 {
    (timestamp << TIMESTAMP_SHIFT) | ((shard_id as u64) << SHARD_SHIFT) | sequence as u64
}

static GLOBAL_FACTORY: OnceLock<GlobalIdFactory> = OnceLock::new();

fn factory() -> &'static GlobalIdFactory {
    GLOBAL_FACTORY.get_or_init(|| GlobalIdFactory::new(0))
}

fn to_base36(mut value: u64) -> String {
    if value == 0 {
        return "0".to_string();
    }
    let mut buf = [0u8; 16];
    let mut idx = buf.len();
    while value > 0 {
        let rem = (value % 36) as usize;
        idx -= 1;
        buf[idx] = BASE36_ALPHABET[rem];
        value /= 36;
    }
    std::str::from_utf8(&buf[idx..]).unwrap().to_string()
}

fn from_base36(value: &str) -> IdResult<u64> {
    u64::from_str_radix(&value.to_uppercase(), 36).map_err(|_| IdError::InvalidBase36)
}

macro_rules! define_id_type {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
        pub struct $name(u64);

        impl $name {
            pub fn generate() -> Self {
                Self::try_generate().expect("id generation failed")
            }

            pub fn try_generate() -> IdResult<Self> {
                factory().next_id().map(Self)
            }

            pub fn try_generate_in_shard(shard_id: u16) -> IdResult<Self> {
                if shard_id > MAX_SHARD_ID {
                    return Err(IdError::ShardOutOfRange(shard_id));
                }
                let temp_factory = GlobalIdFactory::new(shard_id);
                temp_factory.next_id().map(Self)
            }

            pub fn generate_in_shard(shard_id: u16) -> Self {
                Self::try_generate_in_shard(shard_id).expect("id generation failed")
            }

            pub fn from_raw(raw: u64) -> IdResult<Self> {
                GlobalIdFactory::validate_raw(raw)?;
                Ok(Self(raw))
            }

            pub fn new_with_raw_id(raw: u64) -> IdResult<Self> {
                Self::from_raw(raw)
            }

            pub fn new(raw: u64) -> Self {
                Self::from_raw(raw).expect("invalid raw id")
            }

            pub fn from_base36(value: &str) -> IdResult<Self> {
                let raw = from_base36(value)?;
                Self::from_raw(raw)
            }

            pub const fn from_raw_unchecked(raw: u64) -> Self {
                Self(raw)
            }

            pub fn into_inner(self) -> u64 {
                self.0
            }

            pub fn as_u64(&self) -> u64 {
                self.0
            }

            pub fn to_base36(&self) -> String {
                to_base36(self.0)
            }

            pub fn components(&self) -> IdComponents {
                GlobalIdFactory::validate_raw(self.0).expect("id components invalid")
            }

            pub fn timestamp_ms(&self) -> u64 {
                self.components().timestamp_ms
            }

            pub fn shard_id(&self) -> u16 {
                self.components().shard_id
            }

            pub fn sequence(&self) -> u16 {
                self.components().sequence
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::generate()
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                f.write_str(&self.to_base36())
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.to_base36())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct IdVisitor;

                impl<'de> de::Visitor<'de> for IdVisitor {
                    type Value = $name;

                    fn expecting(&self, f: &mut Formatter) -> fmt::Result {
                        f.write_str("a base36 string or u64 representing an id")
                    }

                    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
                    where
                        E: de::Error,
                    {
                        $name::from_raw(value).map_err(E::custom)
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
                    where
                        E: de::Error,
                    {
                        $name::from_base36(value).map_err(E::custom)
                    }
                }

                deserializer.deserialize_any(IdVisitor)
            }
        }

        impl From<$name> for u64 {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl TryFrom<u64> for $name {
            type Error = IdError;

            fn try_from(value: u64) -> IdResult<Self> {
                $name::from_raw(value)
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(s: &str) -> IdResult<Self> {
                $name::from_base36(s)
            }
        }
    };
}

define_id_type!(TenantId);
define_id_type!(HumanId);
define_id_type!(AIId);
define_id_type!(GroupId);
define_id_type!(ToolId);
define_id_type!(MessageId);
define_id_type!(SessionId);
define_id_type!(EventId);
define_id_type!(AwarenessCycleId);
define_id_type!(InferenceCycleId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base36_roundtrip() {
        let id = TenantId::from_raw_unchecked(0x1234_5678_90AB);
        let b36 = id.to_base36();
        let parsed = TenantId::from_base36(&b36).unwrap();
        assert_eq!(id.into_inner(), parsed.into_inner());
    }

    #[test]
    fn validation_checks_layout() {
        let raw = compose_raw_id(100, 2, 3);
        let components = GlobalIdFactory::validate_raw(raw).unwrap();
        assert_eq!(components.timestamp_ms, 100);
        assert_eq!(components.shard_id, 2);
        assert_eq!(components.sequence, 3);
    }
}
