use crate::i18n::tr;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::All,
            Self::All => Self::One,
            Self::One => Self::Off,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => tr("Повтор выключен"),
            Self::All => tr("Повтор всей очереди"),
            Self::One => tr("Повтор одного трека"),
        }
    }
}

/// Shared icon-only playback mode; changing it never starts playback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackMode {
    Ordered,
    Shuffle,
    Endless,
}
impl PlaybackMode {
    pub fn from_flags(shuffle: bool, autoplay: bool) -> Self {
        if autoplay {
            Self::Endless
        } else if shuffle {
            Self::Shuffle
        } else {
            Self::Ordered
        }
    }
    pub fn next(self) -> Self {
        match self {
            Self::Ordered => Self::Shuffle,
            Self::Shuffle => Self::Endless,
            Self::Endless => Self::Ordered,
        }
    }
    pub fn flags(self) -> (bool, bool) {
        match self {
            Self::Ordered => (false, false),
            Self::Shuffle => (true, false),
            Self::Endless => (true, true),
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Self::Ordered => "icons/order.svg",
            Self::Shuffle => "icons/shuffle.svg",
            Self::Endless => "icons/autoplay-star.svg",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Ordered => tr("По порядку"),
            Self::Shuffle => tr("Перемешивание"),
            Self::Endless => tr("Перемешивание с автопродолжением"),
        }
    }
}

/// Playback order is independent of the library and playlist order in SQLite.
#[derive(Clone, Debug, Default)]
pub struct PlaybackQueue {
    original: Vec<i64>,
    order: Vec<i64>,
    cursor: usize,
    shuffled: bool,
    repeat: RepeatMode,
    ignored: HashSet<i64>,
}

impl PlaybackQueue {
    pub fn is_ignored(&self, id: i64) -> bool {
        self.ignored.contains(&id)
    }
    /// Skip newly ignored playback immediately; preserve an explicit replay on refresh.
    pub fn set_ignored(&mut self, ignored: HashSet<i64>) -> bool {
        let current = self.current();
        let skip_current =
            current.is_some_and(|id| ignored.contains(&id) && !self.ignored.contains(&id));
        self.ignored = ignored;
        let remove: Vec<_> = self
            .order
            .iter()
            .copied()
            .filter(|id| self.ignored.contains(id) && (Some(*id) != current || skip_current))
            .collect();
        for id in remove {
            self.remove(id);
        }
        if skip_current
            && self.current().is_none()
            && self.repeat == RepeatMode::All
            && !self.order.is_empty()
        {
            self.cursor = 0;
        }
        skip_current
    }

    pub fn current(&self) -> Option<i64> {
        self.order.get(self.cursor).copied()
    }
    pub fn upcoming(&self) -> &[i64] {
        self.order
            .get(self.cursor.saturating_add(1)..)
            .unwrap_or_default()
    }
    pub fn shuffled(&self) -> bool {
        self.shuffled
    }
    pub fn contains(&self, id: i64) -> bool {
        self.order.contains(&id)
    }

    pub fn start(&mut self, ids: impl IntoIterator<Item = i64>, selected: i64) {
        self.original.clear();
        let mut seen = HashSet::new();
        for id in ids {
            if (id == selected || !self.ignored.contains(&id)) && seen.insert(id) {
                self.original.push(id);
            }
        }
        self.order = self.original.clone();
        self.cursor = self
            .order
            .iter()
            .position(|id| *id == selected)
            .unwrap_or(0);
        if self.shuffled && self.current().is_some() {
            self.order.swap(0, self.cursor);
            self.cursor = 0;
            self.order[1..].shuffle(&mut rand::rng());
        }
    }

    pub fn advance(&mut self) -> Option<i64> {
        self.skip_failed();
        if self.current().is_none() && self.repeat == RepeatMode::All && !self.order.is_empty() {
            self.cursor = 0;
        }
        self.current()
    }

    /// Decoder failures must never loop endlessly, even with repeat enabled.
    pub fn skip_failed(&mut self) -> Option<i64> {
        if let Some(id) = self.current().filter(|id| self.ignored.contains(id)) {
            self.remove(id);
        } else {
            self.cursor = self.cursor.saturating_add(1).min(self.order.len());
        }
        self.current()
    }

    pub fn finish_track(&mut self) -> Option<i64> {
        if self.repeat == RepeatMode::One
            && self.current().is_some_and(|id| !self.ignored.contains(&id))
        {
            self.current()
        } else {
            self.advance()
        }
    }

    pub fn set_repeat(&mut self, repeat: RepeatMode) {
        self.repeat = repeat;
    }

    pub fn previous(&mut self) -> Option<i64> {
        if let Some(id) = self.current().filter(|id| self.ignored.contains(id)) {
            self.remove(id);
        }
        self.cursor = self.cursor.saturating_sub(1);
        self.current()
    }

    pub fn select(&mut self, id: i64) -> bool {
        if !self.contains(id) {
            return false;
        }
        if self.current() != Some(id)
            && let Some(current) = self.current().filter(|id| self.ignored.contains(id))
        {
            self.remove(current);
        }
        if let Some(index) = self.order.iter().position(|entry| *entry == id) {
            self.cursor = index;
            true
        } else {
            false
        }
    }

    pub fn set_shuffle(&mut self, shuffled: bool) {
        self.shuffled = shuffled;
        let start = (self.cursor + 1).min(self.order.len());
        if shuffled {
            self.order[start..].shuffle(&mut rand::rng());
        } else {
            let positions: HashMap<_, _> = self
                .original
                .iter()
                .enumerate()
                .map(|(index, id)| (*id, index))
                .collect();
            self.order[start..].sort_by_key(|id| positions.get(id).copied());
        }
    }

    pub fn enqueue(&mut self, id: i64) -> bool {
        if self.ignored.contains(&id) || self.contains(id) {
            return false;
        }
        self.original.push(id);
        self.order.push(id);
        true
    }

    /// If the current track is removed, its successor occupies the same cursor.
    pub fn remove(&mut self, id: i64) -> bool {
        let was_current = self.current() == Some(id);
        if let Some(index) = self.order.iter().position(|entry| *entry == id) {
            self.order.remove(index);
            if index < self.cursor {
                self.cursor -= 1;
            }
        }
        self.original.retain(|entry| *entry != id);
        was_current
    }

    /// Refill only the exhausted tail. Prefer tracks absent from this listening session.
    pub fn extend_from_library(&mut self, pool: &[i64], enabled: bool) -> bool {
        if !enabled || self.repeat != RepeatMode::Off || !self.upcoming().is_empty() {
            return false;
        }
        let Some(current) = self.current() else {
            return false;
        };
        let mut candidates: Vec<_> = pool
            .iter()
            .copied()
            .filter(|id| !self.ignored.contains(id) && !self.contains(*id))
            .collect();
        if candidates.is_empty() {
            candidates = pool
                .iter()
                .copied()
                .filter(|id| *id != current && !self.ignored.contains(id))
                .collect();
            if candidates.is_empty() {
                return false;
            }
            // Bound history when starting another pass through the library.
            self.start([current], current);
        }
        candidates.shuffle(&mut rand::rng());
        let mut changed = false;
        for id in candidates.into_iter().take(20) {
            changed |= self.enqueue(id);
        }
        changed
    }

    pub fn clear(&mut self) {
        self.original.clear();
        self.order.clear();
        self.cursor = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignored_tracks_are_excluded_from_context_enqueue_and_autoplay() {
        let mut queue = PlaybackQueue::default();
        queue.set_ignored(HashSet::from([2, 4]));
        queue.start([1, 2, 3, 4], 1);
        assert_eq!(queue.upcoming(), [3]);
        assert!(!queue.enqueue(2));
        queue.advance();
        assert!(queue.extend_from_library(&[1, 2, 3, 4, 5], true));
        assert_eq!(queue.upcoming(), [5]);
        queue.advance();
        assert!(queue.extend_from_library(&[1, 2, 3, 4, 5], true));
        assert!(queue.upcoming().iter().all(|id| *id == 1 || *id == 3));
        queue.set_shuffle(true);
        assert!(!queue.contains(2) && !queue.contains(4));
    }

    #[test]
    fn newly_ignored_current_skips_immediately_and_repeat_never_returns_to_it() {
        let mut queue = PlaybackQueue::default();
        queue.start([1, 2, 3], 2);
        queue.set_repeat(RepeatMode::One);
        assert!(queue.set_ignored(HashSet::from([2])));
        assert_eq!(queue.current(), Some(3));
        assert!(!queue.contains(2));
        queue.set_repeat(RepeatMode::All);
        assert_eq!(queue.advance(), Some(1));
        assert_eq!(queue.advance(), Some(3));
        assert!(queue.set_ignored(HashSet::from([2, 3])));
        assert_eq!(queue.current(), Some(1));
        assert!(queue.set_ignored(HashSet::from([1, 2, 3])));
        assert_eq!(queue.finish_track(), None);
    }

    #[test]
    fn explicit_ignored_playback_survives_refresh_but_leaves_no_repeat_or_history_entry() {
        let mut queue = PlaybackQueue::default();
        queue.set_ignored(HashSet::from([2]));
        queue.start([1, 2, 3], 2);
        assert_eq!(queue.current(), Some(2));
        assert!(!queue.set_ignored(HashSet::from([2])));
        queue.set_repeat(RepeatMode::One);
        assert_eq!(queue.finish_track(), Some(3));
        assert_eq!(queue.previous(), Some(1));
        queue.start([1, 2, 3], 2);
        assert!(queue.select(3));
        assert!(!queue.contains(2));
        queue.start([1, 2, 3], 2);
        assert_eq!(queue.previous(), Some(1));
        queue.set_ignored(HashSet::new());
        assert!(queue.enqueue(2));
        queue.clear();
        queue.set_ignored(HashSet::from([2]));
        queue.clear();
        assert!(!queue.enqueue(2));
    }

    #[test]
    fn autoplay_prefers_unheard_tracks_and_obeys_repeat_and_disabled_modes() {
        let mut queue = PlaybackQueue::default();
        queue.start([1, 2], 1);
        assert!(!queue.extend_from_library(&[1, 2, 3, 4], true));
        queue.advance();
        assert!(!queue.extend_from_library(&[1, 2, 3, 4], false));
        queue.set_repeat(RepeatMode::All);
        assert!(!queue.extend_from_library(&[1, 2, 3, 4], true));
        queue.set_repeat(RepeatMode::One);
        assert!(!queue.extend_from_library(&[1, 2, 3, 4], true));
        queue.set_repeat(RepeatMode::Off);
        assert!(queue.extend_from_library(&[1, 2, 3, 4], true));
        let mut upcoming = queue.upcoming().to_vec();
        upcoming.sort();
        assert_eq!(upcoming, [3, 4]);
        assert_eq!(queue.current(), Some(2));
    }

    #[test]
    fn autoplay_can_recycle_a_library_without_repeating_current_or_growing_forever() {
        let mut queue = PlaybackQueue::default();
        queue.start([1, 2], 2);
        for _ in 0..100 {
            let current = queue.current().unwrap();
            assert!(queue.extend_from_library(&[1, 2], true));
            assert_eq!(queue.upcoming(), [if current == 1 { 2 } else { 1 }]);
            queue.advance();
        }
        queue.start([1], 1);
        assert!(!queue.extend_from_library(&[1], true));
        queue.clear();
        assert!(!queue.extend_from_library(&[1, 2], true));
    }

    #[test]
    fn repeat_handles_end_single_manual_next_empty_and_failures() {
        let mut queue = PlaybackQueue::default();
        queue.set_repeat(RepeatMode::All);
        assert_eq!(queue.finish_track(), None);
        queue.start([1, 2], 2);
        assert_eq!(queue.finish_track(), Some(1));
        queue.set_repeat(RepeatMode::One);
        assert_eq!(queue.finish_track(), Some(1));
        assert_eq!(queue.advance(), Some(2));
        assert_eq!(queue.finish_track(), Some(2));
        assert_eq!(queue.skip_failed(), None);
        queue.set_repeat(RepeatMode::All);
        queue.start([1], 1);
        assert_eq!(queue.finish_track(), Some(1));
        queue.clear();
        assert_eq!(queue.finish_track(), None);
    }
    #[test]
    fn follows_selected_context_and_stops_at_the_end() {
        let mut queue = PlaybackQueue::default();
        queue.start([10, 20, 30], 20);
        assert_eq!(queue.current(), Some(20));
        assert_eq!(queue.upcoming(), [30]);
        assert_eq!(queue.advance(), Some(30));
        assert_eq!(queue.advance(), None);
        assert_eq!(queue.previous(), Some(30));
        assert!(queue.select(10));
        assert_eq!(queue.previous(), Some(10));
    }
    #[test]
    fn shuffle_keeps_current_track_and_restores_remaining_source_order() {
        let mut queue = PlaybackQueue::default();
        queue.start(1..=8, 2);
        queue.set_shuffle(true);
        assert_eq!(queue.current(), Some(2));
        let mut remaining = queue.upcoming().to_vec();
        remaining.sort();
        assert_eq!(remaining, (3..=8).collect::<Vec<_>>());
        queue.set_shuffle(false);
        assert_eq!(queue.upcoming(), (3..=8).collect::<Vec<_>>());
        queue.set_shuffle(true);
        queue.start(1..=8, 5);
        assert_eq!(queue.current(), Some(5));
        assert_eq!(queue.upcoming().len(), 7);
    }
    #[test]
    fn deleting_current_and_past_entries_keeps_the_correct_successor() {
        let mut queue = PlaybackQueue::default();
        queue.start([1, 2, 3, 4], 2);
        assert!(!queue.remove(1));
        assert_eq!(queue.current(), Some(2));
        assert!(queue.remove(2));
        assert_eq!(queue.current(), Some(3));
        assert!(!queue.remove(4));
        assert!(queue.upcoming().is_empty());
        assert!(queue.remove(3));
        assert_eq!(queue.current(), None);
    }
    #[test]
    fn appending_after_the_end_resumes_at_the_new_track_without_duplicates() {
        let mut queue = PlaybackQueue::default();
        queue.start([1], 1);
        queue.advance();
        assert!(queue.enqueue(2));
        assert_eq!(queue.current(), Some(2));
        assert!(!queue.enqueue(2));
        queue.clear();
        assert!(queue.enqueue(3));
        assert_eq!(queue.current(), Some(3));
    }
}
