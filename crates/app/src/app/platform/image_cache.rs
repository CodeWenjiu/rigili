use std::{collections::HashMap, sync::Arc};

use gpui_kit::*;

// Decoded thumbnails dominate RSS while scrolling, so bound the cache by
// decoded bytes instead of entries; this also frees the matching GPU texture.
const BUDGET_BYTES: usize = 64 * 1024 * 1024;
// Fast scrolling can queue many decodes before they finish; cap the metadata
// and cancel the oldest pending loads as well.
const MAX_ENTRIES: usize = 128;

struct ImageCacheState(Entity<LruImageCache>);

impl Global for ImageCacheState {}

pub(crate) fn init(cx: &mut App) {
    let cache = LruImageCache::new(cx);
    cx.set_global(ImageCacheState(cache));
}

pub(crate) fn get(cx: &App) -> Entity<LruImageCache> {
    cx.global::<ImageCacheState>().0.clone()
}

pub(crate) struct LruImageCache {
    entries: HashMap<u64, Entry>,
    pending: Vec<u64>,
    tick: u64,
    bytes: usize,
}

struct Entry {
    item: ImageCacheItem,
    used: u64,
    bytes: usize,
}

impl LruImageCache {
    pub(crate) fn new(cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            cx.on_release(|cache: &mut Self, cx| {
                for (_, entry) in std::mem::take(&mut cache.entries) {
                    if let Some(Ok(image)) = entry.item.get() {
                        cx.drop_image(image, None);
                    }
                }
            })
            .detach();

            Self {
                entries: HashMap::new(),
                pending: Vec::new(),
                tick: 0,
                bytes: 0,
            }
        })
    }

    fn account_pending(&mut self) {
        self.pending.retain(|key| {
            let Some(entry) = self.entries.get_mut(key) else {
                return false;
            };
            if entry.bytes == 0
                && let Some(Ok(image)) = entry.item.get()
            {
                entry.bytes = decoded_bytes(&image);
                self.bytes += entry.bytes;
            }
            entry.bytes == 0
        });
    }

    fn evict(&mut self, current: u64, window: &mut Window, cx: &mut App) {
        if self.bytes <= BUDGET_BYTES && self.entries.len() <= MAX_ENTRIES {
            return;
        }
        self.account_pending();

        while (self.bytes > BUDGET_BYTES || self.entries.len() > MAX_ENTRIES)
            && self.entries.len() > 1
        {
            let Some(victim) = self
                .entries
                .iter()
                .filter(|(key, _)| **key != current)
                .min_by_key(|(_, entry)| entry.used)
                .map(|(key, _)| *key)
            else {
                break;
            };
            let Some(entry) = self.entries.remove(&victim) else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(entry.bytes);
            if let Some(Ok(image)) = entry.item.get() {
                cx.drop_image(image, Some(window));
            }
        }
    }
}

impl ImageCache for LruImageCache {
    fn load(
        &mut self,
        resource: &Resource,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let key = hash(resource);
        self.tick += 1;
        let used = self.tick;
        let entry = self.entries.entry(key).or_insert_with(|| {
            self.pending.push(key);
            Entry {
                item: ImageCacheItem::new(resource, cx),
                used,
                bytes: 0,
            }
        });
        entry.used = used;
        let loaded = entry.item.use_image(window);
        if entry.bytes == 0
            && let Some(Ok(image)) = &loaded
        {
            entry.bytes = decoded_bytes(image);
            self.bytes += entry.bytes;
        }

        self.evict(key, window, cx);
        loaded
    }
}

fn decoded_bytes(image: &RenderImage) -> usize {
    (0..image.frame_count())
        .filter_map(|index| image.as_bytes(index))
        .map(<[u8]>::len)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::decoded_bytes;
    use gpui_kit::RenderImage;
    use image::{Frame, Rgba, RgbaImage};

    #[test]
    fn counts_decoded_pixels() {
        let frame = Frame::new(RgbaImage::from_pixel(40, 30, Rgba([0, 0, 0, 0])));
        let image = RenderImage::new([frame]);

        assert_eq!(decoded_bytes(&image), 40 * 30 * 4);
    }
}
