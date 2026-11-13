use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Forest,
    Onyx,
    PickMe,
    Cosmic,
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: u32,
    pub panel: u32,
    pub border: u32,
    pub text: u32,
    pub muted: u32,
    pub accent: u32,
    pub active: u32,
    pub hover_border: u32,
    pub button: u32,
    pub button_hover: u32,
    pub hero: u32,
    pub input: u32,
}

impl Theme {
    pub fn logo_path(self) -> &'static str {
        match self {
            Self::Forest => "shard/forest.svg",
            Self::Onyx => "shard/onyx.svg",
            Self::PickMe => "shard/pick_me.svg",
            Self::Cosmic => "shard/cosmic.svg",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Forest => "Лес",
            Self::Onyx => "Оникс",
            Self::PickMe => "Пикми",
            Self::Cosmic => "Космос",
        }
    }
    pub fn palette(self) -> Palette {
        match self {
            Self::Forest => Palette {
                bg: 0x101412,
                panel: 0x151a17,
                border: 0x29322c,
                text: 0xeaf0e9,
                muted: 0x88958b,
                accent: 0xb5ed80,
                active: 0x19221b,
                hover_border: 0x435448,
                button: 0x242e26,
                button_hover: 0x303d32,
                hero: 0x243522,
                input: 0x191e1c,
            },
            Self::Onyx => Palette {
                bg: 0x090a0c,
                panel: 0x111215,
                border: 0x282a30,
                text: 0xe9e9ee,
                muted: 0x9596a2,
                accent: 0xced0e0,
                active: 0x191b21,
                hover_border: 0x4a4d5b,
                button: 0x212329,
                button_hover: 0x30333c,
                hero: 0x22252f,
                input: 0x191a20,
            },
            Self::PickMe => Palette {
                bg: 0x20141d,
                panel: 0x2b1c27,
                border: 0x513246,
                text: 0xffedf6,
                muted: 0xb995a9,
                accent: 0xf2a8ce,
                active: 0x342231,
                hover_border: 0x86536f,
                button: 0x452c3e,
                button_hover: 0x58364d,
                hero: 0x56314c,
                input: 0x34222f,
            },
            Self::Cosmic => Palette {
                bg: 0x0c0e20,
                panel: 0x14162d,
                border: 0x303454,
                text: 0xeeeaff,
                muted: 0x989ac0,
                accent: 0xb3a2ff,
                active: 0x20223d,
                hover_border: 0x646594,
                button: 0x292b49,
                button_hover: 0x383b60,
                hero: 0x302b59,
                input: 0x1d2039,
            },
        }
    }
}
