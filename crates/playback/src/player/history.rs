use crate::request::PlaybackSource;

const HISTORY_LIMIT: usize = 50;

#[derive(Default)]
pub(super) struct History {
    entries: Vec<PlaybackSource>,
    index: usize,
}

impl History {
    pub(super) fn open(&mut self, source: PlaybackSource) {
        if let Some(current) = self.entries.get_mut(self.index) {
            match (current, source.clone()) {
                (
                    PlaybackSource::Video {
                        bvid: current_bvid,
                        cid: current_cid,
                    },
                    PlaybackSource::Video { bvid, cid },
                ) if *current_bvid == bvid => {
                    if cid.is_some() {
                        *current_cid = cid;
                    }
                    return;
                }
                (
                    PlaybackSource::Live {
                        room_id: current_room,
                    },
                    PlaybackSource::Live { room_id },
                ) if *current_room == room_id => return,
                _ => {}
            }
        }

        self.entries.truncate(self.index + 1);
        self.entries.push(source);
        self.index = self.entries.len() - 1;
        self.trim();
    }

    pub(super) fn remember_cid(&mut self, cid: u64) {
        if let Some(PlaybackSource::Video {
            cid: current_cid, ..
        }) = self.entries.get_mut(self.index)
        {
            *current_cid = Some(cid);
        }
    }

    pub(super) fn back(&mut self) -> Option<PlaybackSource> {
        if self.index == 0 {
            return None;
        }
        self.index -= 1;
        self.entries.get(self.index).cloned()
    }

    pub(super) fn forward(&mut self) -> Option<PlaybackSource> {
        if self.index + 1 >= self.entries.len() {
            return None;
        }
        self.index += 1;
        self.entries.get(self.index).cloned()
    }

    pub(super) fn can_back(&self) -> bool {
        self.index > 0
    }

    pub(super) fn can_forward(&self) -> bool {
        self.index + 1 < self.entries.len()
    }

    fn trim(&mut self) {
        if self.entries.len() <= HISTORY_LIMIT {
            return;
        }
        let excess = self.entries.len() - HISTORY_LIMIT;
        self.entries.drain(..excess);
        self.index = self.index.saturating_sub(excess);
    }
}

#[cfg(test)]
mod tests {
    use super::{HISTORY_LIMIT, History};
    use crate::request::PlaybackSource;

    fn video(bvid: &str, cid: Option<u64>) -> PlaybackSource {
        PlaybackSource::Video {
            bvid: bvid.into(),
            cid,
        }
    }

    #[test]
    fn drops_forward_entries_when_opening_something_new() {
        let mut history = History::default();
        history.open(video("BV1", None));
        history.open(video("BV2", None));
        history.open(video("BV3", None));

        assert_eq!(history.back(), Some(video("BV2", None)));
        assert!(history.can_forward());

        history.open(video("BV4", None));
        assert!(!history.can_forward());
        assert_eq!(history.back(), Some(video("BV2", None)));
    }

    #[test]
    fn remembers_the_page_of_the_current_video() {
        let mut history = History::default();
        history.open(video("BV1", None));
        history.open(video("BV2", Some(10)));
        history.remember_cid(11);

        assert_eq!(history.back(), Some(video("BV1", None)));
        assert_eq!(history.forward(), Some(video("BV2", Some(11))));
    }

    #[test]
    fn reopening_the_current_video_does_not_add_history() {
        let mut history = History::default();
        history.open(video("BV1", Some(1)));
        history.open(video("BV1", Some(2)));

        assert!(!history.can_back());
        assert_eq!(history.back(), None);
    }

    #[test]
    fn caps_history_length() {
        let mut history = History::default();
        for index in 0..HISTORY_LIMIT + 10 {
            history.open(video(&format!("BV{index}"), None));
        }

        let mut steps = 0;
        while history.back().is_some() {
            steps += 1;
        }
        assert_eq!(steps, HISTORY_LIMIT - 1);
    }
}
