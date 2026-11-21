use std::{collections::HashMap, sync::Arc};

use gpui_kit::*;

// Decoded thumbnails dominate RSS while scrolling, so bound the cache by
// decoded bytes instead of entries; this also frees the matching GPU texture.
// The grid retunes the budget from what it draws: a fixed budget was smaller
// than one screenful on HiDPI, so scrolling evicted what it had just decoded.
const COVER_MIN_BUDGET_BYTES: usize = 32 * 1024 * 1024;
const COVER_MAX_BUDGET_BYTES: usize = 64 * 1024 * 1024;
// Until a grid tunes itself, keep enough entries that the byte budget is what
// binds; small tiles would otherwise hit the entry cap first.
const COVER_DEFAULT_ENTRIES: usize = 256;
const COVER_MIN_ENTRIES: usize = 32;
const COVER_MAX_ENTRIES: usize = 512;
// Avatars are many and cheap after resizing, but they must not evict covers.
const AVATAR_BUDGET_BYTES: usize = 16 * 1024 * 1024;
const AVATAR_MAX_ENTRIES: usize = 512;
// The playback page's related list draws small tiles of its own. Sharing the
// grid's tier let a handful of 240px covers evict what a grid still had on
// screen, which read as the grid reloading from scratch.
const RELATED_BUDGET_BYTES: usize = 8 * 1024 * 1024;
const RELATED_MAX_ENTRIES: usize = 64;
// A panel's emote sets are a few hundred tiny pictures that users browse while
// composing. Sharing the avatar tier let comment avatars evict what the picker
// had just shown, so opening it again refetched those images; an account's
// static set fits here instead. The byte budget still binds on animated sets,
// which decode to frames times a picture.
const EMOTE_BUDGET_BYTES: usize = 32 * 1024 * 1024;
const EMOTE_MAX_ENTRIES: usize = 1024;
// Comment pictures are user-uploaded photos and GIFs: the list requests small
// thumbnails and the viewer a 1280px copy, so bytes bind long before the entry
// cap. Animated thumbnails decode every frame, which needs more room than the
// still pictures this tier used to hold.
const COMMENT_PICTURE_BUDGET_BYTES: usize = 64 * 1024 * 1024;
const COMMENT_PICTURE_MAX_ENTRIES: usize = 128;
// The viewer draws one picture at a time. Sharing the list's tier made a large
// animated picture evict the thumbnails behind it, which immediately reloaded
// and evicted the picture again; a separate tier keeps both stable.
const COMMENT_PREVIEW_BUDGET_BYTES: usize = 32 * 1024 * 1024;
const COMMENT_PREVIEW_MAX_ENTRIES: usize = 4;

const TIER_COUNT: usize = 6;

#[repr(usize)]
#[derive(Clone, Copy)]
enum Tier {
    Covers,
    Avatars,
    Related,
    Emotes,
    CommentPictures,
    CommentPreviews,
}

impl Tier {
    const ALL: [Self; TIER_COUNT] = [
        Self::Covers,
        Self::Avatars,
        Self::Related,
        Self::Emotes,
        Self::CommentPictures,
        Self::CommentPreviews,
    ];

    fn limits(self) -> (usize, usize) {
        match self {
            Self::Covers => (COVER_MIN_BUDGET_BYTES, COVER_DEFAULT_ENTRIES),
            Self::Avatars => (AVATAR_BUDGET_BYTES, AVATAR_MAX_ENTRIES),
            Self::Related => (RELATED_BUDGET_BYTES, RELATED_MAX_ENTRIES),
            Self::Emotes => (EMOTE_BUDGET_BYTES, EMOTE_MAX_ENTRIES),
            Self::CommentPictures => (COMMENT_PICTURE_BUDGET_BYTES, COMMENT_PICTURE_MAX_ENTRIES),
            Self::CommentPreviews => (COMMENT_PREVIEW_BUDGET_BYTES, COMMENT_PREVIEW_MAX_ENTRIES),
        }
    }

    fn cached(self, cx: &App) -> Entity<LruImageCache> {
        cx.global::<ImageCacheState>().tiers[self as usize].clone()
    }
}

struct ImageCacheState {
    tiers: [Entity<LruImageCache>; TIER_COUNT],
}

impl Global for ImageCacheState {}

pub fn init(cx: &mut App) {
    let tiers = Tier::ALL.map(|tier| {
        let (budget_bytes, max_entries) = tier.limits();
        LruImageCache::with_limits(cx, budget_bytes, max_entries)
    });
    cx.set_global(ImageCacheState { tiers });
}

pub fn get(cx: &App) -> Entity<LruImageCache> {
    Tier::Covers.cached(cx)
}

pub fn avatars(cx: &App) -> Entity<LruImageCache> {
    Tier::Avatars.cached(cx)
}

/// The playback page's related tiles, kept apart from the grid tier.
pub fn related(cx: &App) -> Entity<LruImageCache> {
    Tier::Related.cached(cx)
}

/// Emote images, kept apart from the avatars that share the comment list.
pub fn emotes(cx: &App) -> Entity<LruImageCache> {
    Tier::Emotes.cached(cx)
}

/// Pictures posted in comments, kept apart from the emotes beside them.
pub fn comment_pictures(cx: &App) -> Entity<LruImageCache> {
    Tier::CommentPictures.cached(cx)
}

/// Full-size pictures opened from a comment, kept apart from their thumbnails.
pub fn comment_previews(cx: &App) -> Entity<LruImageCache> {
    Tier::CommentPreviews.cached(cx)
}

/// Sizes the cover tier against what the grid actually draws.
///
/// `bytes_each` is one decoded tile, `visible` the cards on screen; the budget
/// keeps a screen and a half so a scroll back does not re-decode.
pub fn tune(cx: &mut App, window: &mut Window, visible: usize, bytes_each: usize) {
    let (budget_bytes, max_entries) = cover_budget(visible, bytes_each);
    let covers = get(cx);
    covers.update(cx, |cache, cx| {
        cache.set_limits(budget_bytes, max_entries, window, cx);
    });
}

pub fn cover_budget(visible: usize, bytes_each: usize) -> (usize, usize) {
    let bytes_each = bytes_each.max(1);
    let needed = visible.max(1).saturating_mul(bytes_each).saturating_mul(3) / 2;
    let budget_bytes = needed.clamp(COVER_MIN_BUDGET_BYTES, COVER_MAX_BUDGET_BYTES);
    let max_entries = (budget_bytes / bytes_each).clamp(COVER_MIN_ENTRIES, COVER_MAX_ENTRIES);
    (budget_bytes, max_entries)
}

/// How many covers the tier can still take before it starts evicting. A
/// look-ahead wider than this evicts what it just started, so the next frame
/// re-requests the same URLs; callers use it to bound their prefetch.
pub fn room(cx: &mut App) -> usize {
    let covers = get(cx);
    covers.update(cx, |cache, _| cache.room())
}

/// Starts a cover load outside the paint path, so a scroll finds it decoded.
/// Cheap when the tier already holds it: a hash lookup and no work.
pub fn prefetch(cx: &mut App, window: &mut Window, uri: &str) {
    let resource = Resource::Uri(SharedUri::from(uri.to_owned()));
    let covers = get(cx);
    covers.update(cx, |cache, cx| {
        cache.load(&resource, window, cx);
    });
}

pub struct LruImageCache {
    entries: HashMap<u64, Entry>,
    pending: Vec<u64>,
    tick: u64,
    bytes: usize,
    budget_bytes: usize,
    max_entries: usize,
}

struct Entry {
    item: ImageCacheItem,
    used: u64,
    bytes: usize,
}

impl LruImageCache {
    fn with_limits(cx: &mut App, budget_bytes: usize, max_entries: usize) -> Entity<Self> {
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
                budget_bytes,
                max_entries,
            }
        })
    }

    fn account_pending(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        self.pending.retain(|key| {
            let Some(entry) = self.entries.get_mut(key) else {
                return false;
            };
            if entry.bytes != 0 {
                return false;
            }
            match entry.item.get() {
                Some(Ok(image)) => {
                    entry.bytes = decoded_bytes(&image);
                    self.bytes += entry.bytes;
                    false
                }
                Some(Err(_)) => false,
                None => true,
            }
        });
    }

    fn evict(&mut self, keep: Option<u64>, window: &mut Window, cx: &mut App) {
        self.account_pending();
        if self.bytes <= self.budget_bytes && self.entries.len() <= self.max_entries {
            return;
        }

        while (self.bytes > self.budget_bytes || self.entries.len() > self.max_entries)
            && self.entries.len() > 1
        {
            let Some(victim) = self
                .entries
                .iter()
                .filter(|(key, _)| Some(**key) != keep)
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

    fn room(&mut self) -> usize {
        self.account_pending();
        if self.bytes > self.budget_bytes {
            0
        } else {
            self.max_entries.saturating_sub(self.entries.len())
        }
    }

    fn set_limits(
        &mut self,
        budget_bytes: usize,
        max_entries: usize,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.budget_bytes == budget_bytes && self.max_entries == max_entries {
            return;
        }
        self.budget_bytes = budget_bytes;
        self.max_entries = max_entries;
        self.evict(None, window, cx);
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

        self.evict(Some(key), window, cx);
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
    use super::{Tier, cover_budget, decoded_bytes};

    #[test]
    fn tiers_are_indexed_in_declaration_order() {
        for (index, tier) in Tier::ALL.iter().enumerate() {
            assert_eq!(*tier as usize, index);
        }
    }
    use gpui_kit::RenderImage;
    use image::{Frame, Rgba, RgbaImage};

    #[test]
    fn counts_decoded_pixels() {
        let frame = Frame::new(RgbaImage::from_pixel(40, 30, Rgba([0, 0, 0, 0])));
        let image = RenderImage::new([frame]);

        assert_eq!(decoded_bytes(&image), 40 * 30 * 4);
    }

    #[test]
    fn sizes_the_cover_budget_from_the_screen() {
        let megabyte = 1024 * 1024;

        let (budget, entries) = cover_budget(12, 512 * 1024);
        assert_eq!(budget, 32 * megabyte);
        assert_eq!(entries, 64);

        let (budget, _) = cover_budget(12, 2 * megabyte);
        assert_eq!(budget, 36 * megabyte);

        let (budget, _) = cover_budget(60, 2 * megabyte);
        assert_eq!(budget, 64 * megabyte);

        let (budget, entries) = cover_budget(0, 0);
        assert_eq!(budget, 32 * megabyte);
        assert_eq!(entries, 512);
    }
}
