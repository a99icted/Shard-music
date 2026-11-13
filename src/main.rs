#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(windows)]
mod desktop;
mod input;
mod player;
mod ui;

use gpui::{App, AppContext, Application, Bounds, WindowBounds, WindowOptions, px, size};
use shard::{db::Database, services::application_directory};

fn main() {
    let database = application_directory().and_then(Database::open);
    let database = match database {
        Ok(db) => db,
        Err(error) => {
            rfd::MessageDialog::new()
                .set_title("Shard — ошибка запуска")
                .set_description(format!("{error:#}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
            return;
        }
    };
    Application::new()
        .with_assets(shard::branding::Assets)
        .run(move |cx: &mut App| {
            input::bind_keys(cx);
            ui::bind_keys(cx);
            let bounds = Bounds::centered(None, size(px(1380.), px(820.)), cx);
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(760.), px(520.))),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Shard".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| ui::MusicApp::new(database, window, cx));
                    #[cfg(windows)]
                    if let Err(error) = desktop::install(window, cx, view.read(cx).player()) {
                        // Keep ordinary window-close behavior if the notification area is unavailable.
                        rfd::MessageDialog::new()
                            .set_title("Shard — трей недоступен")
                            .set_description(format!(
                                "{error:#}\nКрестик окна будет завершать приложение."
                            ))
                            .show();
                    }
                    view
                },
            );
            if let Err(error) = result {
                rfd::MessageDialog::new()
                    .set_title("Shard — ошибка окна")
                    .set_description(format!("{error:#}"))
                    .show();
                cx.quit();
            }
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}
