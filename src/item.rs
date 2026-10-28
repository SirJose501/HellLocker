use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Stratagem,
    Booster,
}

impl ItemKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Stratagem => "stratagem",
            Self::Booster => "booster",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StratagemCategory {
    Offensive,
    Supply,
    Defensive,
}

impl StratagemCategory {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Offensive => "offensive",
            Self::Supply => "supply",
            Self::Defensive => "defensive",
        }
    }
}

/// HellLocker: which attire slots a save or apply covers. Weapons and the grenade are
/// always covered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttireChoice {
    pub helmet: bool,
    pub armor: bool,
    pub cape: bool,
}

impl AttireChoice {
    pub const ALL: Self = Self {
        helmet: true,
        armor: true,
        cape: true,
    };

    pub fn includes(self, kind: EquipmentKind) -> bool {
        match kind {
            EquipmentKind::Helmet => self.helmet,
            EquipmentKind::Armor => self.armor,
            EquipmentKind::Cape => self.cape,
            _ => true,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentKind {
    Helmet,
    Armor,
    Cape,
    Primary,
    Secondary,
    Throwable,
}

impl EquipmentKind {
    /// HellLocker: helmet, armor and cape, which the attire boxes can leave out.
    #[cfg(test)]
    pub const fn is_attire(self) -> bool {
        matches!(self, Self::Helmet | Self::Armor | Self::Cape)
    }

    /// HellLocker: the slots a save or apply covers.
    pub fn selected(attire: AttireChoice) -> Vec<Self> {
        Self::ALL
            .into_iter()
            .filter(|kind| attire.includes(*kind))
            .collect()
    }

    pub const fn has_categories(self) -> bool {
        !matches!(self, Self::Helmet | Self::Cape)
    }

    pub const fn list_columns(self) -> usize {
        match self {
            Self::Primary | Self::Secondary => 2,
            _ => 3,
        }
    }

    pub const ALL: [Self; 6] = [
        Self::Helmet,
        Self::Armor,
        Self::Cape,
        Self::Primary,
        Self::Secondary,
        Self::Throwable,
    ];

    /// Index in the six-card entry, also used for in-memory equipment slots.
    pub fn index(self) -> usize {
        match self {
            Self::Helmet => 0,
            Self::Armor => 1,
            Self::Cape => 2,
            Self::Primary => 3,
            Self::Secondary => 4,
            Self::Throwable => 5,
        }
    }
}
