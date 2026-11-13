use gpui::{AssetSource, Result, SharedString};
use std::borrow::Cow;

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
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
