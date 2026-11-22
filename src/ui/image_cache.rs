use super::*;
use std::collections::HashMap;

struct Entry {
    item: ImageCacheItem,
    used: u64,
}

/// Bound decoded images and explicitly release their GPU atlas tiles on eviction.
pub(super) struct ArtworkCache {
    entries: HashMap<Resource, Entry>,
    frame: u64,
}

impl ArtworkCache {
    #[allow(dead_code)]
    pub(super) fn diagnostics(&mut self) -> (usize, usize) {
        let bytes = self
            .entries
            .values_mut()
            .filter_map(|entry| entry.item.get())
            .filter_map(Result::ok)
            .map(|image| {
                (0..image.frame_count())
                    .filter_map(|i| image.as_bytes(i))
                    .map(<[u8]>::len)
                    .sum::<usize>()
            })
            .sum();
        (self.entries.len(), bytes)
    }
    pub fn new(cx: &mut App) -> Entity<Self> {
        let entity = cx.new(|_| Self {
            entries: HashMap::new(),
            frame: 0,
        });
        cx.observe_release(&entity, |cache, cx| {
            for (_, mut entry) in cache.entries.drain() {
                if let Some(Ok(image)) = entry.item.get() {
                    cx.drop_image(image, None);
                }
            }
        })
        .detach();
        entity
    }

    pub fn begin_frame(&mut self, window: &mut Window, cx: &mut App) {
        self.frame += 1;
        let mut bytes: usize = self
            .entries
            .values_mut()
            .filter_map(|e| e.item.get())
            .filter_map(Result::ok)
            .map(|image| {
                (0..image.frame_count())
                    .filter_map(|i| image.as_bytes(i))
                    .map(<[u8]>::len)
                    .sum::<usize>()
            })
            .sum();
        while self.entries.len() > 96 || bytes > 32 * 1024 * 1024 {
            let Some(key) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            let mut entry = self.entries.remove(&key).unwrap();
            if let Some(Ok(image)) = entry.item.get() {
                bytes = bytes.saturating_sub(
                    (0..image.frame_count())
                        .filter_map(|i| image.as_bytes(i))
                        .map(<[u8]>::len)
                        .sum::<usize>(),
                );
                cx.drop_image(image, Some(window));
            }
        }
    }

    pub fn clear(&mut self, window: &mut Window, cx: &mut App) {
        for (_, mut entry) in self.entries.drain() {
            if let Some(Ok(image)) = entry.item.get() {
                cx.drop_image(image, Some(window));
            }
        }
    }

    pub fn remove_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut App) {
        if let Some(mut entry) = self.entries.remove(&Resource::from(path))
            && let Some(Ok(image)) = entry.item.get()
        {
            cx.drop_image(image, Some(window));
        }
    }
}

impl ImageCache for ArtworkCache {
    fn load(
        &mut self,
        source: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        if let Some(entry) = self.entries.get_mut(source) {
            entry.used = self.frame;
            return entry.item.get();
        }
        use futures::FutureExt;
        let future = AssetLogger::<ImageAssetLoader>::load(source.clone(), cx);
        let task = cx.background_executor().spawn(future).shared();
        self.entries.insert(
            source.clone(),
            Entry {
                item: ImageCacheItem::Loading(task.clone()),
                used: self.frame,
            },
        );
        let entity = window.current_view();
        window
            .spawn(cx, async move |cx| {
                let _ = task.await;
                cx.on_next_frame(move |_, cx| cx.notify(entity));
            })
            .detach();
        None
    }
}
