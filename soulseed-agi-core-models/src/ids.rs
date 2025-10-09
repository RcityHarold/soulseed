use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

macro_rules! newtype_u64 {
    ($name:ident) => {
        #[derive(
            Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize,
        )]
        pub struct $name(pub u64);

        impl $name {
            #[inline]
            pub const fn new(raw: u64) -> Self {
                Self(raw)
            }

            #[inline]
            pub const fn new_with_raw_id(raw: u64) -> Self {
                Self(raw)
            }

            #[inline]
            pub const fn into_inner(self) -> u64 {
                self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

newtype_u64!(TenantId);
newtype_u64!(HumanId);
newtype_u64!(AIId);
newtype_u64!(GroupId);
newtype_u64!(ToolId);
newtype_u64!(MessageId);
newtype_u64!(SessionId);
newtype_u64!(EventId);
newtype_u64!(CycleId);
newtype_u64!(InferenceCycleId);
