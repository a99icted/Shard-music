use super::*;
use shard::youtube::search::{self as backend, ResultKind, SearchFilters, SearchResult};
use std::collections::{HashMap, HashSet};

fn compact_search_count(count: &str) -> String {
    let Ok(value) = count.parse::<u64>() else {
        return count.to_owned();
    };
    for (unit, suffix) in [(1_000_000_000, "B"), (1_000_000, "M"), (10_000, "K")] {
        if value >= unit {
            let divisor = if suffix == "K" { 1000 } else { unit };
            return format!("{:.1}{suffix}", value as f64 / divisor as f64);
        }
    }
    count.to_owned()
}

/// Small session caches avoid repeating network requests while editing queries.
/// Expired entries and the least recently used query are discarded on insertion.
struct QueryCache<T> {
    entries: Vec<(String, Instant, T)>,
    limit: usize,
    ttl: Duration,
}
impl<T: Clone> QueryCache<T> {
    fn new(limit: usize) -> Self {
        Self {
            entries: vec![],
            limit,
            ttl: Duration::from_secs(120),
        }
    }
    fn get(&mut self, query: &str) -> Option<T> {
        self.entries.retain(|(_, at, _)| at.elapsed() < self.ttl);
        let index = self.entries.iter().position(|(key, _, _)| key == query)?;
        let entry = self.entries.remove(index);
        let value = entry.2.clone();
        self.entries.push(entry);
        Some(value)
    }
    fn insert(&mut self, query: String, value: T) {
        self.entries
            .retain(|(key, at, _)| key != &query && at.elapsed() < self.ttl);
        if self.entries.len() >= self.limit {
            self.entries.remove(0);
        }
        self.entries.push((query, Instant::now(), value));
    }
}

impl QueryCache<Vec<String>> {
    fn matching_suggestions(&self, query: &str) -> Vec<String> {
        let prefix = query.trim().to_lowercase();
        let mut seen = HashSet::new();
        self.entries
            .iter()
            .rev()
            .filter(|(_, at, _)| at.elapsed() < self.ttl)
            .flat_map(|(_, _, items)| items)
            .filter(|item| item.to_lowercase().starts_with(&prefix))
            .filter(|item| seen.insert(item.as_str()))
            .take(10)
            .cloned()
            .collect()
    }
}

pub(super) struct SearchUi {
    pub online: bool,
    mode_epoch: u64,
    filters: SearchFilters,
    filters_open: bool,
    filter_bounds: Option<Bounds<Pixels>>,
    filter_debounce: Option<Task<()>>,
    input: String,
    suggestions: Vec<String>,
    suggestion_cache: QueryCache<Vec<String>>,
    result_cache: QueryCache<Vec<SearchResult>>,
    suggestion_generation: u64,
    debounce: Option<Task<()>>,
    search_generation: u64,
    cancel: Option<Arc<AtomicBool>>,
    loading: bool,
    submitted: bool,
    error: Option<String>,
    results: Vec<SearchResult>,
    expanded: HashSet<String>,
    collections: HashMap<String, Vec<SearchResult>>,
    collection_loading: HashSet<String>,
    collection_errors: HashMap<String, String>,
    rows: Rc<Vec<(SearchResult, bool)>>,
    list: ListState,
    matches: HashMap<String, i64>,
    art: HashMap<String, PathBuf>,
    art_requested: HashSet<String>,
    playback_generation: u64,
    playback_cancel: Option<Arc<AtomicBool>>,
    preparing: Option<String>,
    launching: Option<i64>,
    pub previews: HashMap<i64, (Track, SearchResult)>,
    next_preview: i64,
}
impl SearchUi {
    pub fn new() -> Self {
        Self {
            online: false,
            mode_epoch: 0,
            filters: SearchFilters::default(),
            filters_open: false,
            filter_bounds: None,
            filter_debounce: None,
            input: String::new(),
            suggestions: vec![],
            suggestion_cache: QueryCache {
                ttl: Duration::from_secs(600),
                ..QueryCache::new(64)
            },
            result_cache: QueryCache::new(8),
            suggestion_generation: 0,
            debounce: None,
            search_generation: 0,
            cancel: None,
            loading: false,
            submitted: false,
            error: None,
            results: vec![],
            expanded: HashSet::new(),
            collections: HashMap::new(),
            collection_loading: HashSet::new(),
            collection_errors: HashMap::new(),
            rows: Rc::new(vec![]),
            list: ListState::new(0, ListAlignment::Top, px(240.)),
            matches: HashMap::new(),
            art: HashMap::new(),
            art_requested: HashSet::new(),
            playback_generation: 0,
            playback_cancel: None,
            preparing: None,
            launching: None,
            previews: HashMap::new(),
            next_preview: -1,
        }
    }
    fn cancel_search(&mut self) {
        self.filter_debounce = None;
        self.search_generation += 1;
        self.suggestion_generation += 1;
        self.debounce = None;
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.loading = false;
        self.collection_loading.clear();
    }
    pub fn dismiss_suggestions(&mut self) -> bool {
        if self.filters_open {
            self.filters_open = false;
            return true;
        }
        if self.suggestions.is_empty() {
            return false;
        }
        self.suggestions.clear();
        self.suggestion_generation += 1;
        self.debounce = None;
        true
    }
}
impl Drop for SearchUi {
    fn drop(&mut self) {
        if let Some(cancel) = &self.cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(cancel) = &self.playback_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl MusicApp {
    fn toggle_search_filter(&mut self, kind: ResultKind, cx: &mut Context<Self>) {
        let filters = &mut self.discovery.filters;
        match kind {
            ResultKind::Track => filters.tracks = !filters.tracks,
            ResultKind::Album => filters.albums = !filters.albums,
            ResultKind::Playlist => filters.playlists = !filters.playlists,
        }
        self.discovery.cancel_search();
        self.discovery.suggestions.clear();
        let filters = self.discovery.filters;
        self.discovery
            .results
            .retain(|result| filters.allows(result.kind));
        self.rebuild_search_rows();
        if !self.search.read(cx).value().trim().is_empty() {
            if filters.is_empty() {
                self.discovery.submitted = true;
            } else {
                self.discovery.loading = true;
                self.discovery.filter_debounce = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(280))
                        .await;
                    let _ = this.update(cx, |app, cx| app.submit_online_search(cx));
                }));
            }
        }
        cx.notify();
    }

    pub(super) fn search_changed(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value();
        if query == self.discovery.input {
            cx.notify();
            return;
        }
        self.discovery.input = query.clone();
        self.discovery.suggestion_generation += 1;
        self.discovery.debounce = None;
        self.discovery.suggestions.clear();
        if !self.discovery.online {
            cx.notify();
            return;
        }
        if query.trim().is_empty() {
            self.discovery.cancel_search();
            self.discovery.results.clear();
            self.discovery.collections.clear();
            self.discovery.submitted = false;
            self.rebuild_search_rows();
            cx.notify();
            return;
        }
        if self.discovery.filters.is_empty() {
            cx.notify();
            return;
        }
        let generation = self.discovery.suggestion_generation;
        if let Some(items) = self.discovery.suggestion_cache.get(&query) {
            self.discovery.suggestions = items;
            cx.notify();
            return;
        }
        // Display relevant cached completions immediately while the fresh request
        // is debounced. Typing one more letter no longer empties the dropdown.
        self.discovery.suggestions = self.discovery.suggestion_cache.matching_suggestions(&query);
        self.discovery.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let cache_key = query.clone();
            let task = cx
                .background_executor()
                .spawn(async move { backend::suggestions(&query) });
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                if app.discovery.online && app.discovery.suggestion_generation == generation {
                    // Suggestions are optional: a provider failure must not disable search.
                    if let Ok(items) = result {
                        app.discovery
                            .suggestion_cache
                            .insert(cache_key, items.clone());
                        app.discovery.suggestions = items;
                    }
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    pub(super) fn set_search_mode(&mut self, online: bool, cx: &mut Context<Self>) {
        if self.discovery.online == online {
            return;
        }
        self.discovery.cancel_search();
        self.cancel_pending_preview();
        self.discovery.online = online;
        self.discovery.filters_open = false;
        self.discovery.mode_epoch += 1;
        self.discovery.expanded.clear();
        self.rebuild_search_rows();
        if online {
            self.home_open = false;
        }
        self.discovery.suggestions.clear();
        self.discovery.input.clear();
        self.search.update(cx, |input, cx| {
            input.set_placeholder(if online {
                tr("Поиск на YouTube")
            } else {
                tr("Поиск по названию или исполнителю")
            });
            cx.notify();
        });
        self.search_changed(cx);
        cx.notify();
    }

    pub(super) fn submit_online_search(&mut self, cx: &mut Context<Self>) {
        let query = self.search.read(cx).value().trim().to_owned();
        if query.is_empty() {
            return;
        }
        // Input notifications may arrive after Enter/click. Do not restart
        // autocomplete for the query that has just been submitted.
        self.discovery.input = self.search.read(cx).value();
        self.discovery.cancel_search();
        self.discovery.suggestions.clear();
        self.discovery.loading = true;
        self.discovery.submitted = true;
        self.discovery.error = None;
        self.discovery.results.clear();
        self.discovery.expanded.clear();
        self.discovery.collections.clear();
        self.discovery.collection_errors.clear();
        let filters = self.discovery.filters;
        let cache_key = filters.cache_key(&query);
        if filters.is_empty() {
            self.discovery.loading = false;
            self.rebuild_search_rows();
            cx.notify();
            return;
        }
        if let Some(results) = self.discovery.result_cache.get(&cache_key) {
            self.discovery.results = results;
            self.discovery.loading = false;
            self.rebuild_search_rows();
            cx.notify();
            return;
        }
        self.rebuild_search_rows();
        let generation = self.discovery.search_generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.discovery.cancel = Some(cancel.clone());
        let root = self.data_directory.clone();
        let task = cx
            .background_executor()
            .spawn(async move { backend::search_filtered(&root, &query, filters, &cancel) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                if app.discovery.search_generation != generation {
                    return;
                }
                app.discovery.loading = false;
                match result {
                    Ok(results) => {
                        app.discovery
                            .result_cache
                            .insert(cache_key, results.clone());
                        app.discovery.results = results;
                        app.rebuild_search_rows();
                    }
                    Err(error) => app.discovery.error = Some(format!("{error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn update_search_matches(&mut self) {
        let mut results = self.discovery.results.clone();
        results.extend(self.discovery.collections.values().flatten().cloned());
        self.discovery.matches = backend::collection_matches(&self.tracks, &results);
    }
    fn rebuild_search_rows(&mut self) {
        let mut rows = Vec::new();
        for item in &self.discovery.results {
            rows.push((item.clone(), false));
            if self.discovery.expanded.contains(&item.id) {
                if let Some(tracks) = self.discovery.collections.get(&item.id) {
                    rows.extend(tracks.iter().cloned().map(|t| (t, true)));
                }
            }
        }
        self.discovery.list.reset(rows.len());
        self.discovery.rows = Rc::new(rows);
        self.update_search_matches();
    }

    fn expand_search_collection(&mut self, item: SearchResult, cx: &mut Context<Self>) {
        if !self.discovery.expanded.insert(item.id.clone()) {
            self.discovery.expanded.remove(&item.id);
            self.rebuild_search_rows();
            cx.notify();
            return;
        }
        if self.discovery.collections.contains_key(&item.id) {
            self.rebuild_search_rows();
            cx.notify();
            return;
        }
        if !self.discovery.collection_loading.insert(item.id.clone()) {
            cx.notify();
            return;
        }
        self.discovery.collection_errors.remove(&item.id);
        let generation = self.discovery.search_generation;
        let cancel = self
            .discovery
            .cancel
            .clone()
            .unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        let root = self.data_directory.clone();
        let id = item.id.clone();
        let task = cx
            .background_executor()
            .spawn(async move { backend::collection(&root, &item, &cancel) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                if app.discovery.search_generation != generation {
                    return;
                }
                app.discovery.collection_loading.remove(&id);
                match result {
                    Ok(tracks) => {
                        app.discovery.collections.insert(id, tracks);
                    }
                    Err(error) => {
                        app.discovery
                            .collection_errors
                            .insert(id, format!("{error:#}"));
                    }
                }
                app.rebuild_search_rows();
                cx.notify();
            });
        })
        .detach();
        self.rebuild_search_rows();
        cx.notify();
    }

    fn queue_search_download(&mut self, item: &SearchResult, cx: &mut Context<Self>) {
        if self.search_download_pending(item) {
            return;
        }
        if item.kind != ResultKind::Track {
            if let Some(tracks) = self.discovery.collections.get(&item.id).cloned() {
                if !tracks.is_empty() {
                    self.pending_downloads
                        .push_back(downloads::DownloadRequest {
                            url: item.url.clone(),
                            collection: Some(backend::CollectionDownload {
                                collection: item.clone(),
                                tracks,
                            }),
                        });
                    self.start_next_download(cx);
                    cx.notify();
                }
            }
            return;
        }
        if self.discovery.matches.contains_key(&item.id) {
            return;
        }
        self.pending_downloads.push_back(item.url.clone().into());
        self.start_next_download(cx);
        cx.notify();
    }
    fn search_download_pending(&self, item: &SearchResult) -> bool {
        let contains = |collection: &backend::CollectionDownload| {
            collection.collection.id == item.id
                || collection.tracks.iter().any(|track| track.id == item.id)
        };
        self.active_download.as_ref() == Some(&item.url)
            || self
                .active_download_collection
                .as_ref()
                .is_some_and(contains)
            || self.pending_downloads.iter().any(|request| {
                request.url == item.url || request.collection.as_ref().is_some_and(contains)
            })
    }

    fn play_search_results(&mut self, items: Vec<SearchResult>, cx: &mut Context<Self>) {
        let Some(first) = items.first().cloned() else {
            return;
        };
        self.cancel_pending_preview();
        if items.len() == 1 {
            if let Some(id) = self.discovery.matches.get(&first.id).copied() {
                if let Some(track) = self.tracks.iter().find(|t| t.id == id).cloned() {
                    self.playback_context = tr("Ваша библиотека").into();
                    self.playback_playlist = None;
                    self.playback_query.clear();
                    if self.player.state().track_id == Some(id) {
                        self.player.toggle();
                    } else if let Ok(playback) = self.playback_track(&track) {
                        self.player.start(vec![playback], id);
                    }
                    cx.notify();
                    return;
                }
            }
            if self
                .discovery
                .previews
                .get(&self.player.state().track_id.unwrap_or(0))
                .is_some_and(|(_, r)| r.id == first.id)
            {
                self.player.toggle();
                cx.notify();
                return;
            }
        }
        let generation = self.discovery.playback_generation;
        self.discovery.preparing = Some(first.id.clone());
        let cancel = Arc::new(AtomicBool::new(false));
        self.discovery.playback_cancel = Some(cancel.clone());
        let root = self.data_directory.clone();
        let task = cx
            .background_executor()
            .spawn(async move { backend::resolve_stream(&root, &first, &cancel) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                if app.discovery.playback_generation != generation {
                    return;
                }
                match result {
                    Ok(info) => {
                        let mut items = items;
                        if items[0].artist.is_empty() {
                            items[0].artist = info.artist.unwrap_or_default();
                        }
                        items[0].duration_ms = info.duration_ms.or(items[0].duration_ms);
                        items[0].play_count = info.play_count.or(items[0].play_count.clone());
                        let updated = &items[0];
                        for result in app
                            .discovery
                            .results
                            .iter_mut()
                            .chain(app.discovery.collections.values_mut().flatten())
                        {
                            if result.id == updated.id {
                                *result = updated.clone();
                            }
                        }
                        // Keep the list state and scroll offset while refreshing metadata.
                        app.discovery.rows = Rc::new(
                            app.discovery
                                .rows
                                .iter()
                                .map(|(result, nested)| {
                                    (
                                        if result.id == updated.id {
                                            updated.clone()
                                        } else {
                                            result.clone()
                                        },
                                        *nested,
                                    )
                                })
                                .collect(),
                        );
                        app.discovery.previews.clear();
                        let queue: Vec<_> = items
                            .into_iter()
                            .map(|item| {
                                let id = app.discovery.next_preview;
                                app.discovery.next_preview -= 1;
                                app.discovery.previews.insert(
                                    id,
                                    (
                                        Track {
                                            id,
                                            title: item.title.clone(),
                                            artist: item.artist.clone(),
                                            duration_ms: item.duration_ms,
                                            source_url: Some(item.url.clone()),
                                            file_path: String::new(),
                                            created_at: String::new(),
                                        },
                                        item.clone(),
                                    ),
                                );
                                (id, item)
                            })
                            .collect();
                        app.playback_context = tr("Поиск на YouTube").into();
                        app.playback_playlist = None;
                        app.playback_query.clear();
                        app.player.stream(
                            app.data_directory.clone(),
                            queue.clone(),
                            queue[0].0,
                            info.url,
                        );
                        app.discovery.launching = Some(queue[0].0);
                    }
                    Err(error) => {
                        app.discovery.preparing = None;
                        app.error = Some(format!("{error:#}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn find_playback_track(&self, id: i64) -> Option<&Track> {
        self.track_index
            .get(&id)
            .and_then(|i| self.tracks.get(*i))
            .or_else(|| self.discovery.previews.get(&id).map(|p| &p.0))
    }
    pub(super) fn cancel_pending_preview(&mut self) {
        if let Some(cancel) = self.discovery.playback_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.discovery.playback_generation += 1;
        self.discovery.preparing = None;
        self.discovery.launching = None;
    }
    pub(super) fn finish_preview_start(&mut self) {
        if let Some(id) = self.discovery.launching {
            let state = self.player.state();
            if state.track_id == Some(id) || state.error.is_some() {
                self.discovery.launching = None;
                self.discovery.preparing = None;
            }
        }
    }

    fn request_search_artwork(&mut self, item: &SearchResult, cx: &mut Context<Self>) {
        let Some(url) = item.thumbnail.clone() else {
            return;
        };
        if self.discovery.art.contains_key(&url)
            || !self.discovery.art_requested.insert(url.clone())
        {
            return;
        }
        let root = self.data_directory.join("cache/search-artwork");
        let requested = url.clone();
        let task = cx.background_executor().spawn(async move {
            use sha2::{Digest, Sha256};
            let path = root.join(format!("{:x}.png", Sha256::digest(requested.as_bytes())));
            if !path.is_file() {
                let agent = ureq::Agent::config_builder()
                    .timeout_global(Some(Duration::from_secs(8)))
                    .build()
                    .new_agent();
                let mut response = agent.get(&requested).call()?;
                let bytes = response
                    .body_mut()
                    .with_config()
                    .limit(4 * 1024 * 1024)
                    .read_to_vec()?;
                let image = image::ImageReader::new(std::io::Cursor::new(bytes))
                    .with_guessed_format()?
                    .decode()?
                    .resize_to_fill(128, 128, image::imageops::FilterType::Triangle);
                std::fs::create_dir_all(&root)?;
                let mut staged = tempfile::NamedTempFile::new_in(&root)?;
                image.write_to(staged.as_file_mut(), image::ImageFormat::Png)?;
                staged.persist(&path)?;
            }
            Ok::<_, anyhow::Error>(path)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |app, cx| {
                if let Ok(path) = result {
                    app.discovery.art.insert(url, path);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn search_artwork(&self, item: &SearchResult, size: f32) -> AnyElement {
        let colors = self.colors();
        let mut cover = div()
            .size(px(size))
            .flex_shrink_0()
            .rounded_lg()
            .overflow_hidden()
            .bg(rgb(colors.button))
            .flex()
            .items_center()
            .justify_center();
        if let Some(path) = item
            .thumbnail
            .as_ref()
            .and_then(|url| self.discovery.art.get(url))
        {
            cover = cover.child(
                img(path.clone())
                    .image_cache(&self.images)
                    .size_full()
                    .object_fit(ObjectFit::Cover),
            );
        } else {
            cover = cover.child(
                svg()
                    .path(if item.kind == ResultKind::Track {
                        "icons/music.svg"
                    } else {
                        "icons/playlist.svg"
                    })
                    .size_5()
                    .text_color(rgb(colors.muted)),
            );
        }
        cover.into_any_element()
    }

    pub(super) fn search_spinner(&self, id: impl Into<ElementId>, size: f32) -> AnyElement {
        svg()
            .path("icons/loader.svg")
            .size(px(size))
            .with_animation(
                id,
                Animation::new(Duration::from_millis(850)).repeat(),
                |icon, t| {
                    icon.with_transformation(Transformation::rotate(radians(
                        std::f32::consts::TAU * t,
                    )))
                },
            )
            .into_any_element()
    }

    pub(super) fn search_controls(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let online = self.discovery.online;
        let initial = self.discovery.mode_epoch == 0;
        let toggle = div()
            .id("search-mode-toggle")
            .relative()
            .w(px(76.))
            .h(px(38.))
            .flex_shrink_0()
            .rounded_full()
            .bg(rgb(colors.button))
            .border_1()
            .border_color(rgb(colors.border))
            .child(
                div()
                    .absolute()
                    .top(px(3.))
                    .size(px(30.))
                    .rounded_full()
                    .bg(rgb(colors.active))
                    .with_animation(
                        ("search-mode-slide", self.discovery.mode_epoch),
                        Animation::new(Duration::from_millis(210)).with_easing(ease_out_quint()),
                        move |pill, t| {
                            let target = if online { 40. } else { 4. };
                            let start = if initial {
                                target
                            } else if online {
                                4.
                            } else {
                                40.
                            };
                            pill.left(px(start + (target - start) * t))
                        },
                    ),
            )
            .child(
                div()
                    .relative()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(3.))
                    .children(
                        [
                            (false, "icons/folder.svg", tr("Локальный поиск")),
                            (true, "icons/globe.svg", tr("Поиск в интернете")),
                        ]
                        .into_iter()
                        .map(|(mode, icon, label)| {
                            div()
                                .id(if mode {
                                    "search-online"
                                } else {
                                    "search-local"
                                })
                                .size(px(30.))
                                .rounded_full()
                                .cursor_pointer()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_color(rgb(if mode == online {
                                    colors.accent
                                } else {
                                    colors.muted
                                }))
                                .hover(|s| s.opacity(0.75))
                                .tooltip(move |_, cx| {
                                    cx.new(|_| sidebar::SidebarTooltip(label.into())).into()
                                })
                                .child(svg().path(icon).size_4())
                                .on_click(
                                    cx.listener(move |app, _, _, cx| app.set_search_mode(mode, cx)),
                                )
                        }),
                    ),
            );
        let weak = cx.weak_entity();
        let field = div()
            .relative()
            .flex_1()
            .min_w_0()
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_1()
                    .pr(px(5.))
                    .rounded(px(12.))
                    .border_1()
                    .border_color(rgb(colors.border))
                    .bg(rgb(colors.input))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .child(self.search.clone()),
                    )
                    .children(online.then(|| {
                        div()
                            .id("search-filters-toggle")
                            .relative()
                            .size_8()
                            .flex_shrink_0()
                            .rounded_lg()
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(
                                if self.discovery.filters_open
                                    || self.discovery.filters != SearchFilters::default()
                                {
                                    colors.accent
                                } else {
                                    colors.muted
                                },
                            ))
                            .hover(move |s| s.bg(rgb(colors.button_hover)))
                            .tooltip(|_, cx| {
                                cx.new(|_| sidebar::SidebarTooltip(tr("Фильтры поиска").into()))
                                    .into()
                            })
                            .child(svg().path("icons/list-filter.svg").size(px(19.)))
                            .children((self.discovery.filters != SearchFilters::default()).then(
                                || {
                                    div()
                                        .absolute()
                                        .right(px(3.))
                                        .top(px(3.))
                                        .size(px(4.))
                                        .rounded_full()
                                        .bg(rgb(colors.accent))
                                },
                            ))
                            .child(
                                canvas(
                                    move |bounds, _, cx| {
                                        let _ = weak.update(cx, |app, _| {
                                            app.discovery.filter_bounds = Some(bounds)
                                        });
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .inset_0(),
                            )
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.discovery.filters_open = !app.discovery.filters_open;
                                app.discovery.suggestions.clear();
                                app.discovery.suggestion_generation += 1;
                                app.discovery.debounce = None;
                                cx.notify();
                            }))
                    }))
                    .children(online.then(|| {
                        div()
                            .id("submit-youtube-search")
                            .size_8()
                            .flex_shrink_0()
                            .rounded_lg()
                            .cursor_pointer()
                            .text_color(rgb(colors.accent))
                            .when(self.discovery.filters.is_empty(), |s| s.opacity(0.4))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(move |s| s.bg(rgb(colors.button_hover)))
                            .tooltip(|_, cx| {
                                cx.new(|_| sidebar::SidebarTooltip(tr("Найти").into()))
                                    .into()
                            })
                            .child(if self.discovery.loading {
                                self.search_spinner("search-submit-spinner", 18.)
                            } else {
                                svg().path("icons/search.svg").size_4().into_any_element()
                            })
                            .on_click(cx.listener(|app, _, _, cx| app.submit_online_search(cx)))
                    })),
            )
            .children((online && self.discovery.filters_open).then(|| {
                deferred(
                    div()
                        .id("search-filters-menu")
                        .absolute()
                        .right_0()
                        .top(px(49.))
                        .w(px(220.))
                        .p_1()
                        .rounded_xl()
                        .bg(rgb(colors.panel))
                        .border_1()
                        .border_color(rgb(colors.border))
                        .shadow_lg()
                        .occlude()
                        .flex()
                        .flex_col()
                        .on_mouse_down_out(cx.listener(|app, event: &MouseDownEvent, _, cx| {
                            if !app
                                .discovery
                                .filter_bounds
                                .is_some_and(|bounds| bounds.contains(&event.position))
                            {
                                app.discovery.filters_open = false;
                                cx.notify();
                            }
                        }))
                        .children(
                            [
                                (ResultKind::Track, tr("Треки")),
                                (ResultKind::Album, tr("Альбомы")),
                                (ResultKind::Playlist, tr("Плейлисты")),
                            ]
                            .into_iter()
                            .enumerate()
                            .map(|(index, (kind, label))| {
                                let selected = self.discovery.filters.allows(kind);
                                div()
                                    .id(("search-filter-option", index))
                                    .h(px(38.))
                                    .px_3()
                                    .rounded_lg()
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .text_sm()
                                    .text_color(rgb(colors.text))
                                    .hover(move |s| s.bg(rgb(colors.button_hover)))
                                    .child(
                                        div()
                                            .size(px(18.))
                                            .flex_shrink_0()
                                            .rounded(px(4.))
                                            .border_1()
                                            .border_color(rgb(if selected {
                                                colors.accent
                                            } else {
                                                colors.muted
                                            }))
                                            .bg(rgb(if selected {
                                                colors.accent
                                            } else {
                                                colors.panel
                                            }))
                                            .text_color(rgb(colors.bg))
                                            .text_xs()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .children(selected.then_some("✓")),
                                    )
                                    .child(label)
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        app.toggle_search_filter(kind, cx)
                                    }))
                            }),
                        )
                        .with_animation(
                            "search-filters-appear",
                            Animation::new(Duration::from_millis(130)),
                            |menu, t| menu.opacity(t),
                        ),
                )
                .with_priority(20)
            }));
        div()
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_2()
            .child(toggle)
            .child(field)
            .into_any_element()
    }

    fn online_result_row(
        &mut self,
        item: SearchResult,
        nested: bool,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.request_search_artwork(&item, cx);
        let colors = self.colors();
        let collection = item.kind != ResultKind::Track;
        let expanded = self.discovery.expanded.contains(&item.id);
        let present = self.discovery.matches.contains_key(&item.id);
        let pending = self.search_download_pending(&item);
        let state = self.player.state();
        let playing = state.track_id.is_some_and(|id| {
            self.discovery
                .previews
                .get(&id)
                .is_some_and(|(_, result)| result.id == item.id)
                || self.discovery.matches.get(&item.id) == Some(&id)
        }) && !state.paused;
        let preparing =
            self.discovery.preparing.as_ref() == Some(&item.id) || (playing && state.buffering);
        let click_item = item.clone();
        let download = item.clone();
        let url = item.url.clone();
        let mut row = div()
            .id(("online-row", index))
            .w_full()
            .flex_shrink_0()
            .px_3()
            .py_2()
            .rounded_lg()
            .hover(move |s| s.bg(rgb(colors.button_hover)))
            .flex()
            .items_center()
            .gap_3()
            .when(nested, |s| {
                s.pl_8().border_l_1().border_color(rgb(colors.border))
            })
            .child(self.search_artwork(&item, 48.))
            .child(
                div()
                    .id(("online-title", index))
                    .flex_1()
                    .min_w_0()
                    .cursor_pointer()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_sm()
                            .text_ellipsis()
                            .overflow_hidden()
                            .child(item.title.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .text_color(rgb(colors.muted))
                            .child(
                                div()
                                    .rounded_md()
                                    .px_2()
                                    .bg(rgb(colors.button))
                                    .text_color(rgb(colors.accent))
                                    .child(match item.kind {
                                        ResultKind::Track => tr("Трек"),
                                        ResultKind::Album => tr("Альбом"),
                                        ResultKind::Playlist => tr("Плейлист"),
                                    }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .overflow_hidden()
                                    .child(item.artist.clone()),
                            )
                            .children(item.play_count.as_ref().map(|count| {
                                div().flex_shrink_0().child(format!(
                                    "{} {}",
                                    compact_search_count(count),
                                    tr("прослушиваний")
                                ))
                            })),
                    )
                    .on_click(cx.listener(move |app, _, _, cx| {
                        if collection {
                            app.expand_search_collection(click_item.clone(), cx);
                        } else {
                            app.play_search_results(vec![click_item.clone()], cx);
                        }
                    })),
            );
        if collection {
            let expand = item.clone();
            row = row.child(
                self.icon_button(
                    ("expand-result", index),
                    if expanded { "−" } else { "+" },
                    expanded,
                )
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.expand_search_collection(expand.clone(), cx)
                })),
            );
        } else {
            let play = item.clone();
            row = row
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(colors.muted))
                        .flex_shrink_0()
                        .child(format_duration(item.duration_ms)),
                )
                .child(
                    div()
                        .id(("preview-result", index))
                        .size_8()
                        .rounded_full()
                        .cursor_pointer()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(if playing { colors.accent } else { colors.muted }))
                        .hover(move |s| s.bg(rgb(colors.button_hover)))
                        .child(if preparing {
                            self.search_spinner(("preview-spinner", index), 17.)
                        } else {
                            div()
                                .text_sm()
                                .child(if playing { "Ⅱ" } else { "▶" })
                                .into_any_element()
                        })
                        .tooltip(move |_, cx| {
                            cx.new(|_| {
                                sidebar::SidebarTooltip(
                                    tr(if preparing {
                                        "Подготовка воспроизведения…"
                                    } else if playing {
                                        "Пауза"
                                    } else {
                                        "Слушать"
                                    })
                                    .into(),
                                )
                            })
                            .into()
                        })
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.play_search_results(vec![play.clone()], cx)
                        })),
                )
                .child(
                    div()
                        .id(("download-result", index))
                        .size_8()
                        .flex_shrink_0()
                        .rounded_full()
                        .border_1()
                        .border_color(rgb(if present {
                            colors.accent
                        } else {
                            colors.border
                        }))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(rgb(if present { colors.accent } else { colors.muted }))
                        .hover(move |s| s.bg(rgb(colors.button_hover)))
                        .child(if pending && !present {
                            self.search_spinner(("download-spinner", index), 16.)
                        } else {
                            div()
                                .text_lg()
                                .child(if present { "✓" } else { "+" })
                                .with_animation(
                                    ("download-state", (index * 2 + usize::from(present)) as u64),
                                    Animation::new(Duration::from_millis(180)),
                                    |icon, t| icon.opacity(0.4 + 0.6 * t),
                                )
                                .into_any_element()
                        })
                        .tooltip(move |_, cx| {
                            cx.new(|_| {
                                sidebar::SidebarTooltip(
                                    tr(if present {
                                        "В коллекции"
                                    } else if pending {
                                        "Скачивание…"
                                    } else {
                                        "Скачать в коллекцию"
                                    })
                                    .into(),
                                )
                            })
                            .into()
                        })
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.queue_search_download(&download, cx)
                        })),
                );
        }
        row = row.child(
            div()
                .id(("external-result", index))
                .size_8()
                .flex_shrink_0()
                .rounded_lg()
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(colors.muted))
                .hover(move |s| s.bg(rgb(colors.button_hover)))
                .child(svg().path("icons/external-link.svg").size_4())
                .on_click(move |_, _, cx| cx.open_url(&url)),
        );
        let mut container = div().w_full().flex().flex_col().child(row);
        if collection && expanded {
            if self.discovery.collection_loading.contains(&item.id) {
                container = container.child(
                    div()
                        .ml_6()
                        .p_3()
                        .text_xs()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(self.search_spinner(("collection-spinner", index), 16.))
                        .child(tr("Получаем список треков…")),
                );
            } else if let Some(error) = self.discovery.collection_errors.get(&item.id) {
                container = container.child(div().ml_6().p_3().text_xs().child(error.clone()));
            } else if let Some(tracks) = self.discovery.collections.get(&item.id) {
                let play = tracks.clone();
                let download = item.clone();
                container = container.child(
                    div()
                        .ml_6()
                        .p_2()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_2()
                        .child(
                            self.button(("play-all-results", index), tr("▶  Слушать всё"), true)
                                .w(px(148.))
                                .h(px(40.))
                                .py_0()
                                .px_3()
                                .flex()
                                .items_center()
                                .justify_center()
                                .gap_2()
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.play_search_results(play.clone(), cx)
                                })),
                        )
                        .child(
                            self.button(("download-all-results", index), tr("Скачать всё"), false)
                                .w(px(148.))
                                .h(px(40.))
                                .py_0()
                                .px_3()
                                .flex()
                                .items_center()
                                .justify_center()
                                .gap_2()
                                .child(if pending {
                                    self.search_spinner(("download-collection-spinner", index), 16.)
                                } else {
                                    svg().path("icons/download.svg").size_4().into_any_element()
                                })
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.queue_search_download(&download, cx)
                                })),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(colors.muted))
                                .child(tracks.len().to_string()),
                        ),
                );
            }
        }
        container.into_any_element()
    }

    pub(super) fn online_search_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.colors();
        let mut view = div()
            .size_full()
            .max_w(px(1120.))
            .min_w_0()
            .min_h_0()
            .py_4()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.search_controls(cx));
        if !self.discovery.suggestions.is_empty() {
            view = view.child(
                div()
                    .id("youtube-suggestions")
                    .max_h(px(280.))
                    .overflow_y_scroll()
                    .flex_shrink_0()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(colors.border))
                    .bg(rgb(colors.panel))
                    .children(self.discovery.suggestions.iter().enumerate().map(
                        |(index, suggestion)| {
                            let suggestion = suggestion.clone();
                            div()
                                .id(("youtube-suggestion", index))
                                .px_4()
                                .py_2()
                                .text_sm()
                                .cursor_pointer()
                                .hover(move |s| s.bg(rgb(colors.button_hover)))
                                .child(suggestion.clone())
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.discovery.input = suggestion.clone();
                                    app.search.update(cx, |input, cx| {
                                        input.set_value(&suggestion);
                                        cx.notify();
                                    });
                                    app.submit_online_search(cx);
                                }))
                        },
                    )),
            );
        }
        view = view.children(self.notifications(cx));
        if let Some(error) = &self.discovery.error {
            view = view.child(div().p_3().text_sm().child(error.clone()));
        }
        if self.discovery.rows.is_empty() {
            view = view.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .text_color(rgb(colors.muted))
                    .children(
                        self.discovery
                            .loading
                            .then(|| self.search_spinner("search-results-spinner", 24.)),
                    )
                    .child(if self.discovery.filters.is_empty() {
                        tr("Выберите хотя бы один тип результатов")
                    } else if self.discovery.loading {
                        tr("Ищем на YouTube…")
                    } else if self.discovery.submitted {
                        tr("Ничего не найдено")
                    } else {
                        tr("Найдите треки, альбомы и плейлисты на YouTube")
                    }),
            );
        } else {
            let rows = self.discovery.rows.clone();
            view = view.child(
                list(
                    self.discovery.list.clone(),
                    cx.processor(move |app, index: usize, _, cx| {
                        let (item, nested) = &rows[index];
                        app.online_result_row(item.clone(), *nested, index, cx)
                    }),
                )
                .flex_1()
                .min_h_0(),
            );
        }
        view.into_any_element()
    }
}
