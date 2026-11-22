use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "icons/ban.svg" => include_bytes!("../assets/ban.svg"),
            "icons/trash.svg" => include_bytes!("../assets/trash.svg"),
            "icons/autoplay-star.svg" => include_bytes!("../assets/autoplay-star.svg"),
            "icons/favorites-cover.svg" => include_bytes!("../assets/favorites-cover.svg"),
            "icons/library.svg" => include_bytes!("../assets/library.svg"),
            "icons/order.svg" => include_bytes!("../assets/order.svg"),
            "icons/shuffle-infinity.svg" => include_bytes!("../assets/shuffle-infinity.svg"),
            "icons/library-cover.svg" => include_bytes!("../assets/library-cover.svg"),
            "icons/playlist.svg" => include_bytes!("../assets/playlist.svg"),
            "icons/brand.svg" => include_bytes!("../assets/brand.svg"),
            "icons/settings.svg" => include_bytes!("../assets/settings.svg"),
            "icons/music.svg" => include_bytes!("../assets/music.svg"),
            "icons/play.svg" => include_bytes!("../assets/play.svg"),
            "icons/pause.svg" => include_bytes!("../assets/pause.svg"),
            "icons/next.svg" => include_bytes!("../assets/next.svg"),
            "icons/previous.svg" => include_bytes!("../assets/previous.svg"),
            "icons/close.svg" => include_bytes!("../assets/close.svg"),
            "icons/more.svg" => include_bytes!("../assets/more.svg"),
            "icons/edit.svg" => include_bytes!("../assets/edit.svg"),
            "icons/folder.svg" => include_bytes!("../assets/folder.svg"),
            "icons/download.svg" => include_bytes!("../assets/download.svg"),
            "icons/shuffle.svg" => include_bytes!("../assets/shuffle.svg"),
            "icons/stop.svg" => include_bytes!("../assets/stop.svg"),

            "icons/panel-collapse.svg" => include_bytes!("../assets/panel-collapse.svg"),
            "icons/panel-expand.svg" => include_bytes!("../assets/panel-expand.svg"),
            "icons/copy.svg" => include_bytes!("../assets/copy.svg"),
            "icons/heart.svg" => include_bytes!("../assets/heart.svg"),
            "icons/heart-outline.svg" => include_bytes!("../assets/heart-outline.svg"),
            "shard/forest.svg" => include_bytes!(concat!(env!("OUT_DIR"), "/shard-forest.svg")),
            "shard/onyx.svg" => include_bytes!(concat!(env!("OUT_DIR"), "/shard-onyx.svg")),
            "shard/pick_me.svg" => include_bytes!(concat!(env!("OUT_DIR"), "/shard-pick_me.svg")),
            "shard/cosmic.svg" => include_bytes!(concat!(env!("OUT_DIR"), "/shard-cosmic.svg")),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }
    fn list(&self, _: &str) -> Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visible_pixels(bytes: &[u8]) -> usize {
        let tree = resvg::usvg::Tree::from_data(bytes, &Default::default()).unwrap();
        let mut pixels = resvg::tiny_skia::Pixmap::new(48, 48).unwrap();
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(
                48. / tree.size().width(),
                48. / tree.size().height(),
            ),
            &mut pixels.as_mut(),
        );
        pixels.pixels().iter().filter(|p| p.alpha() > 0).count()
    }

    #[test]
    fn bundled_icons_and_theme_marks_have_visible_pixels() {
        let assets = Assets;
        for name in [
            "autoplay-star",
            "favorites-cover",
            "library",
            "library-cover",
            "playlist",
            "brand",
            "copy",
            "order",
            "shuffle",
            "shuffle-infinity",
            "settings",
            "music",
            "play",
            "pause",
            "next",
            "previous",
            "close",
            "more",
            "edit",
            "folder",
            "download",
            "stop",
            "panel-collapse",
            "panel-expand",
            "heart",
            "heart-outline",
        ] {
            let path = format!("icons/{name}.svg");
            let bytes = assets.load(&path).unwrap().unwrap();
            assert!(visible_pixels(&bytes) > 8, "Invisible asset: {path}");
        }
        for theme in [
            crate::theme::Theme::Forest,
            crate::theme::Theme::Onyx,
            crate::theme::Theme::PickMe,
            crate::theme::Theme::Cosmic,
        ] {
            let bytes = assets.load(theme.logo_path()).unwrap().unwrap();
            assert!(visible_pixels(&bytes) > 8);
        }
    }
}
