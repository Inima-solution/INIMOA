//! Decision state option enum.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Type-safe options for the Decision State system property.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStateOption {
    Proposed,
    Accepted,
    Rejected,
    Superseded,
}

impl TryFrom<&str> for DecisionStateOption {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "Proposed" | "proposed" => Ok(Self::Proposed),
            "Accepted" | "accepted" => Ok(Self::Accepted),
            "Rejected" | "rejected" => Ok(Self::Rejected),
            "Superseded" | "superseded" => Ok(Self::Superseded),
            _ => Err(format!("unknown decision state option: {value}")),
        }
    }
}

impl DecisionStateOption {
    pub const PROPOSED_UUID: Uuid = Uuid::from_u128(0x00000001_0000_0000_0015_000000000001);
    pub const ACCEPTED_UUID: Uuid = Uuid::from_u128(0x00000001_0000_0000_0015_000000000002);
    pub const REJECTED_UUID: Uuid = Uuid::from_u128(0x00000001_0000_0000_0015_000000000003);
    pub const SUPERSEDED_UUID: Uuid = Uuid::from_u128(0x00000001_0000_0000_0015_000000000004);

    pub const fn uuid(self) -> Uuid {
        match self {
            Self::Proposed => Self::PROPOSED_UUID,
            Self::Accepted => Self::ACCEPTED_UUID,
            Self::Rejected => Self::REJECTED_UUID,
            Self::Superseded => Self::SUPERSEDED_UUID,
        }
    }

    pub const fn display_value(self) -> &'static str {
        match self {
            Self::Proposed => "Proposed",
            Self::Accepted => "Accepted",
            Self::Rejected => "Rejected",
            Self::Superseded => "Superseded",
        }
    }

    pub fn from_uuid(uuid: Uuid) -> Option<Self> {
        match uuid {
            value if value == Self::PROPOSED_UUID => Some(Self::Proposed),
            value if value == Self::ACCEPTED_UUID => Some(Self::Accepted),
            value if value == Self::REJECTED_UUID => Some(Self::Rejected),
            value if value == Self::SUPERSEDED_UUID => Some(Self::Superseded),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DecisionStateOption;

    #[test]
    fn uuid_roundtrip_and_wire_names_are_stable() {
        for (option, wire_name) in [
            (DecisionStateOption::Proposed, "Proposed"),
            (DecisionStateOption::Accepted, "Accepted"),
            (DecisionStateOption::Rejected, "Rejected"),
            (DecisionStateOption::Superseded, "Superseded"),
        ] {
            assert_eq!(DecisionStateOption::from_uuid(option.uuid()), Some(option));
            assert_eq!(option.display_value(), wire_name);
            assert_eq!(DecisionStateOption::try_from(wire_name), Ok(option));
        }
    }
}
