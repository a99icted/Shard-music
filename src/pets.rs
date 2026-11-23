//! Small, persisted identifiers; sprite pixels are loaded only by the desktop UI.
use crate::i18n::tr;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorPet {
    #[default]
    None,
    Kitten,
    Frog,
    Pig,
}
impl CursorPet {
    pub const ALL: [Self; 4] = [Self::None, Self::Kitten, Self::Frog, Self::Pig];
    pub fn label(self) -> &'static str {
        match self {
            Self::None => tr("Выключено"),
            Self::Kitten => label("Котёнок", "Kitten"),
            Self::Frog => label("Жабка", "Frog"),
            Self::Pig => label("Поросёнок", "Piglet"),
        }
    }
    #[cfg(feature = "desktop")]
    pub fn asset(self) -> Option<&'static [u8]> {
        match self {
            Self::None => None,
            Self::Kitten => Some(include_bytes!("../assets/pets/cursor-kitten.png")),
            Self::Frog => Some(include_bytes!("../assets/pets/cursor-frog.png")),
            Self::Pig => Some(include_bytes!("../assets/pets/cursor-pig.png")),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressPet {
    #[default]
    None,
    Cat,
    Bird,
    DancingCats,
    Frog,
    Pig,
}
impl ProgressPet {
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::Cat,
        Self::Bird,
        Self::DancingCats,
        Self::Frog,
        Self::Pig,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::None => tr("Выключено"),
            Self::Cat => label("Спящий котик", "Sleeping cat"),
            Self::Bird => label("Птичка в гнезде", "Nesting bird"),
            Self::DancingCats => label("Танцующие котики", "Dancing cats"),
            Self::Frog => label("Пиксельная жабка", "Pixel frog"),
            Self::Pig => label("Поросёнок", "Piglet"),
        }
    }
    pub fn variants(self) -> &'static [PetColor] {
        match self {
            Self::None => &[],
            Self::Pig => &[PetColor::Original, PetColor::Gray],
            Self::Cat => &[
                PetColor::Original,
                PetColor::Gray,
                PetColor::White,
                PetColor::Black,
                PetColor::Headphones,
                PetColor::Grooving,
            ],
            Self::Frog => &[
                PetColor::Original,
                PetColor::Gray,
                PetColor::White,
                PetColor::Black,
                PetColor::Headphones,
                PetColor::Grooving,
                PetColor::Melancholy,
            ],
            Self::Bird | Self::DancingCats => &[
                PetColor::Original,
                PetColor::Gray,
                PetColor::White,
                PetColor::Black,
            ],
        }
    }
    pub fn normalize_color(self, color: PetColor) -> PetColor {
        if self.variants().contains(&color) {
            color
        } else {
            PetColor::Original
        }
    }
    pub fn sleeping(self, variant: PetColor) -> bool {
        (matches!(self, Self::Cat | Self::Bird) && !self.listening(variant))
            || (matches!(self, Self::Frog | Self::Pig) && variant == PetColor::Original)
    }
    pub fn listening(self, variant: PetColor) -> bool {
        matches!(self, Self::Cat | Self::Frog)
            && matches!(variant, PetColor::Headphones | PetColor::Grooving)
    }
    pub fn smoking(self, variant: PetColor) -> bool {
        self == Self::Frog && variant == PetColor::Melancholy
    }
    pub fn variant_label(self, variant: PetColor) -> &'static str {
        if self == Self::Pig {
            return if variant == PetColor::Gray {
                label("Моргает", "Blinking")
            } else {
                label("Спит", "Sleeping")
            };
        }
        if self != Self::Frog {
            return variant.label();
        }
        match variant {
            PetColor::Original => label("Спит", "Sleeping"),
            PetColor::Gray => label("Моргает", "Blinking"),
            PetColor::White => label("Дышит", "Breathing"),
            PetColor::Black => label("Мечтает", "Dreaming"),
            PetColor::Headphones => label("В наушниках", "Headphones"),
            PetColor::Grooving => label("Качает", "Grooving"),
            PetColor::Melancholy => label("Грустит и курит", "Melancholy"),
        }
    }
    #[cfg(feature = "desktop")]
    pub fn variant_asset(self, color: PetColor) -> Option<&'static [u8]> {
        if self == Self::Pig {
            return Some(if color == PetColor::Gray {
                include_bytes!("../assets/pets/pig-blink.png")
            } else {
                include_bytes!("../assets/pets/pig-sleep.png")
            });
        }
        if self == Self::Frog {
            return Some(match color {
                PetColor::Original => include_bytes!("../assets/pets/frog-sleep.png"),
                PetColor::Gray => include_bytes!("../assets/pets/frog-blink.png"),
                PetColor::White => include_bytes!("../assets/pets/frog-breathe.png"),
                PetColor::Black => include_bytes!("../assets/pets/frog-dream.png"),
                PetColor::Headphones => include_bytes!("../assets/pets/frog-headphones.png"),
                PetColor::Grooving => include_bytes!("../assets/pets/frog-grooving.png"),
                PetColor::Melancholy => include_bytes!("../assets/pets/frog-melancholy.png"),
            });
        }
        if self == Self::Cat && color == PetColor::Headphones {
            return Some(include_bytes!("../assets/pets/cat-headphones.png"));
        }
        if self == Self::Cat && color == PetColor::Grooving {
            return Some(include_bytes!("../assets/pets/cat-grooving.png"));
        }
        if self == Self::Bird {
            return Some(match color {
                PetColor::Gray => include_bytes!("../assets/pets/bird-gray.png"),
                PetColor::White => include_bytes!("../assets/pets/bird-white.png"),
                PetColor::Black => include_bytes!("../assets/pets/bird-black.png"),
                PetColor::Original
                | PetColor::Headphones
                | PetColor::Grooving
                | PetColor::Melancholy => {
                    include_bytes!("../assets/pets/bird.png")
                }
            });
        }
        self.asset()
    }
    #[cfg(feature = "desktop")]
    pub fn asset(self) -> Option<&'static [u8]> {
        match self {
            Self::None => None,
            Self::Cat => Some(include_bytes!("../assets/pets/cat.png")),
            Self::Bird => Some(include_bytes!("../assets/pets/bird.png")),
            Self::DancingCats => Some(include_bytes!("../assets/pets/dancing-cats.png")),
            Self::Frog => Some(include_bytes!("../assets/pets/frog-sleep.png")),
            Self::Pig => Some(include_bytes!("../assets/pets/pig-sleep.png")),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PetColor {
    #[default]
    Original,
    Gray,
    White,
    Black,
    Headphones,
    Grooving,
    Melancholy,
}
impl PetColor {
    pub fn label(self) -> &'static str {
        match self {
            Self::Original => label("Исходный", "Original"),
            Self::Gray => label("Серый", "Gray"),
            Self::White => label("Белый", "White"),
            Self::Black => label("Чёрный", "Black"),
            Self::Headphones => label("В наушниках", "Headphones"),
            Self::Grooving => label("Качает", "Grooving"),
            Self::Melancholy => label("Грустит и курит", "Melancholy"),
        }
    }
}

#[cfg(all(test, feature = "desktop"))]
mod tests {
    use super::*;
    use image::GenericImageView;
    #[test]
    fn grooving_heads_keep_feet_fixed_and_headphones_inside_canvas() {
        for pet in [ProgressPet::Cat, ProgressPet::Frog] {
            let atlas = image::load_from_memory(pet.variant_asset(PetColor::Grooving).unwrap())
                .unwrap()
                .to_rgba8();
            let baseline = image::imageops::crop_imm(&atlas, 0, 76, 128, 20).to_image();
            for i in 0..8 {
                assert_eq!(
                    image::imageops::crop_imm(&atlas, i * 128, 76, 128, 20).to_image(),
                    baseline
                );
                for y in 0..4 {
                    assert!(
                        (i * 128..(i + 1) * 128).all(|x| atlas.get_pixel(x, y)[3] == 0),
                        "Headphone band clips the top edge"
                    );
                }
            }
        }
    }
    #[test]
    fn sleeping_loops_keep_a_fixed_baseline_and_clean_edges() {
        for pet in [ProgressPet::Cat, ProgressPet::Bird, ProgressPet::Frog] {
            let atlas = image::load_from_memory(pet.asset().unwrap())
                .unwrap()
                .to_rgba8();
            let mut baselines = vec![];
            for i in 0..8 {
                let frame = image::imageops::crop_imm(&atlas, i * 128, 0, 128, 96);
                let baseline = frame
                    .pixels()
                    .filter(|(_, _, p)| p[3] > 150)
                    .map(|(_, y, _)| y)
                    .max()
                    .unwrap();
                baselines.push(baseline);
            }
            assert!(baselines.iter().all(|y| *y == baselines[0]));
            let spills = atlas
                .pixels()
                .filter(|p| {
                    p[3] > 40
                        && p[0] > p[1].saturating_add(12)
                        && p[2] > p[1].saturating_add(12)
                        && p[2] > 45
                })
                .count();
            assert_eq!(spills, 0, "Magenta fringe remains");
        }
    }
    #[test]
    fn bird_variants_preserve_nest_alpha_and_animation_geometry() {
        let original = image::load_from_memory(ProgressPet::Bird.asset().unwrap())
            .unwrap()
            .to_rgba8();
        for color in [PetColor::Gray, PetColor::White, PetColor::Black] {
            let variant = image::load_from_memory(ProgressPet::Bird.variant_asset(color).unwrap())
                .unwrap()
                .to_rgba8();
            assert_eq!(original.dimensions(), variant.dimensions());
            let mut changed = 0;
            for (x, y, pixel) in original.enumerate_pixels() {
                let other = variant.get_pixel(x, y);
                assert_eq!(pixel[3], other[3]);
                if y >= 77 {
                    assert_eq!(pixel, other, "Nest changed");
                }
                if pixel != other {
                    changed += 1;
                }
            }
            assert!(changed > 5000, "Feather variant is missing");
        }
    }
    #[test]
    fn sprite_atlases_are_small_transparent_and_have_eight_frames() {
        for pet in ProgressPet::ALL {
            for variant in pet.variants() {
                let bytes = pet.variant_asset(*variant).unwrap();
                let image = image::load_from_memory(bytes).unwrap().to_rgba8();
                assert_eq!(image.dimensions(), (128 * 8, 96));
                assert!(image.pixels().any(|p| p[3] == 0));
                assert!(image.pixels().any(|p| p[3] == 255));
                assert!(image.as_raw().len() <= 400 * 1024);
                assert!(bytes.len() < 160 * 1024);
                assert_eq!(pet.normalize_color(*variant), *variant);
            }
        }
    }
}

fn label(ru: &'static str, en: &'static str) -> &'static str {
    if crate::i18n::is_english() { en } else { ru }
}
