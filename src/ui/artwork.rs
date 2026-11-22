use super::*;

impl MusicApp {
    /// Keep migration off the library-loading path. Only one small batch decodes
    /// at a time; navigation and playback can use the metadata immediately.
    pub(super) fn prepare_previews(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        for path in paths {
            if self.thumbnail_pending.insert(path.clone()) {
                self.thumbnail_queue.push_back(path);
            }
        }
        if self.thumbnail_cancel.is_some() || self.thumbnail_queue.is_empty() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.thumbnail_cancel = Some(cancel.clone());
        cx.spawn(async move |this, cx| {
            loop {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(batch) = this.update(cx, |app, _| {
                    let count = app.thumbnail_queue.len().min(8);
                    app.thumbnail_queue.drain(..count).collect::<Vec<_>>()
                }) else {
                    break;
                };
                if batch.is_empty() {
                    break;
                }
                let token = cancel.clone();
                let task = cx.background_executor().spawn(async move {
                    batch
                        .into_iter()
                        .map(|path| {
                            let ready = !token.load(Ordering::Relaxed)
                                && shard::artwork::ensure_thumbnail(&path).is_ok();
                            (path, ready)
                        })
                        .collect::<Vec<_>>()
                });
                let completed = task.await;
                if this
                    .update(cx, |app, cx| {
                        let mut changed = false;
                        for (path, ready) in completed {
                            app.thumbnail_pending.remove(&path);
                            if ready {
                                app.preview_paths.insert(path.clone());
                                app.image_invalidations
                                    .push(shard::artwork::thumbnail_path(&path));
                                changed = true;
                            }
                        }
                        if changed {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
            }
            let _ = this.update(cx, |app, cx| {
                app.thumbnail_cancel = None;
                cx.notify();
            });
        })
        .detach();
    }
}
