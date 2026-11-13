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
            Self::Off => "Повтор выключен",
            Self::All => "Повтор всей очереди",
            Self::One => "Повтор одного трека",
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
}

impl PlaybackQueue {
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
            if seen.insert(id) {
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
        self.cursor = self.cursor.saturating_add(1).min(self.order.len());
        self.current()
    }

    pub fn finish_track(&mut self) -> Option<i64> {
        if self.repeat == RepeatMode::One && self.current().is_some() {
            self.current()
        } else {
            self.advance()
        }
    }

    pub fn set_repeat(&mut self, repeat: RepeatMode) {
        self.repeat = repeat;
    }

    pub fn previous(&mut self) -> Option<i64> {
        self.cursor = self.cursor.saturating_sub(1);
        self.current()
    }

    pub fn select(&mut self, id: i64) -> bool {
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
        if self.contains(id) {
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
