//! HellLocker context tags: which enemy faction and mission type a preset is meant for.
//! `Any` on a preset means "general purpose" and the preset shows under every filter;
//! `Any` as the panel's filter shows every preset.

macro_rules! context_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $key:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
        pub enum $name {
            #[default]
            #[serde(rename = "any")]
            Any,
            $(#[serde(rename = $key)] $variant,)+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[Self::Any, $(Self::$variant),+];

            /// The value written to config.toml.
            pub fn key(self) -> &'static str {
                match self {
                    Self::Any => "any",
                    $(Self::$variant => $key,)+
                }
            }

            /// Position in `ALL`, as the UI's combo boxes use it.
            pub fn index(self) -> i32 {
                Self::ALL.iter().position(|value| *value == self).unwrap_or(0) as i32
            }

            /// Out-of-range indexes fall back to `Any`.
            pub fn from_index(index: i32) -> Self {
                usize::try_from(index)
                    .ok()
                    .and_then(|index| Self::ALL.get(index).copied())
                    .unwrap_or(Self::Any)
            }

            /// Whether a preset tagged `self` belongs in a list filtered by `view`.
            pub fn shows_in(self, view: Self) -> bool {
                view == Self::Any || self == Self::Any || self == view
            }
        }
    };
}

context_enum!(
    /// The enemy faction a preset is for.
    Faction {
        Terminids => "terminids",
        Automatons => "automatons",
        Illuminate => "illuminate",
    }
);

context_enum!(
    /// The mission type a preset is for.
    Mission {
        Standard => "standard",
        Blitz => "blitz",
        Eradicate => "eradicate",
        Defense => "defense",
    }
);

context_enum!(
    /// HellLocker: the enemy sub-faction a preset is for. Each belongs to one faction
    /// (see `faction`), and they are kept grouped by faction in `ALL`, as the panel's
    /// sub-faction boxes list them.
    Subfaction {
        PredatorStrain => "predator_strain",
        SporeBurstStrain => "spore_burst_strain",
        RuptureStrain => "rupture_strain",
        JetBrigade => "jet_brigade",
        IncinerationCorps => "incineration_corps",
        CyborgLegion => "cyborg_legion",
        MindlessMasses => "mindless_masses",
        Appropriators => "appropriators",
        VoteSnatchers => "vote_snatchers",
    }
);

impl Subfaction {
    /// The faction this sub-faction is part of (`Any` for `Any`).
    pub fn faction(self) -> Faction {
        match self {
            Self::Any => Faction::Any,
            Self::PredatorStrain | Self::SporeBurstStrain | Self::RuptureStrain => {
                Faction::Terminids
            }
            Self::JetBrigade | Self::IncinerationCorps | Self::CyborgLegion => Faction::Automatons,
            Self::MindlessMasses | Self::Appropriators | Self::VoteSnatchers => Faction::Illuminate,
        }
    }

    /// This sub-faction when it belongs to `faction`, otherwise `Any`.
    pub fn within(self, faction: Faction) -> Self {
        if faction != Faction::Any && self.faction() == faction {
            self
        } else {
            Self::Any
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subfactions_are_grouped_by_faction_and_reset_outside_it() {
        let parents: Vec<_> = Subfaction::ALL[1..].iter().map(|s| s.faction()).collect();
        let mut sorted = parents.clone();
        sorted.sort_by_key(|f| f.index());
        assert_eq!(parents, sorted);
        assert!(parents.iter().all(|f| *f != Faction::Any));
        assert_eq!(
            Subfaction::JetBrigade.within(Faction::Automatons),
            Subfaction::JetBrigade
        );
        assert_eq!(
            Subfaction::JetBrigade.within(Faction::Terminids),
            Subfaction::Any
        );
        assert_eq!(Subfaction::JetBrigade.within(Faction::Any), Subfaction::Any);
    }

    #[test]
    fn indexes_round_trip_and_out_of_range_is_any() {
        for (index, faction) in Faction::ALL.iter().enumerate() {
            assert_eq!(faction.index(), index as i32);
            assert_eq!(Faction::from_index(index as i32), *faction);
        }
        assert_eq!(Faction::from_index(-1), Faction::Any);
        assert_eq!(Mission::from_index(99), Mission::Any);
    }

    #[test]
    fn general_presets_show_everywhere_and_any_filter_shows_all() {
        assert!(Faction::Any.shows_in(Faction::Terminids));
        assert!(Faction::Terminids.shows_in(Faction::Any));
        assert!(Faction::Terminids.shows_in(Faction::Terminids));
        assert!(!Faction::Terminids.shows_in(Faction::Automatons));
        assert!(!Mission::Blitz.shows_in(Mission::Defense));
    }

    #[test]
    fn keys_match_serde_names() {
        #[derive(serde::Deserialize)]
        struct Row {
            faction: Faction,
            mission: Mission,
        }
        for faction in Faction::ALL {
            for mission in Mission::ALL {
                let text = format!(
                    "faction = \"{}\"\nmission = \"{}\"\n",
                    faction.key(),
                    mission.key()
                );
                let row: Row = toml::from_str(&text).unwrap();
                assert_eq!((row.faction, row.mission), (*faction, *mission));
            }
        }
    }
}
