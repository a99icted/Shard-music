# Shard 0.1.2 — Spotify imports & playlist customization

## English

Shard is a lightweight native music player for Windows, written in Rust.

### New

- Import Spotify tracks, albums, and public playlists. Shard reads public metadata, matches tracks on YouTube, and downloads MP3 audio while preserving titles, artists, and playlist order.
- Try the next suitable YouTube result when a matched video cannot be downloaded.
- Switch the app between English and Russian without restarting.
- Find themes and backgrounds under **Settings → Customization**, and language selection under **Settings → Language**.
- Edit playlist names directly in the pencil dialog. Leaving the name empty keeps the previous one.
- Switch between cover-based and custom header colors; use compact color sliders or enter HEX.
- Edit track titles and artists from the track menu.

### Improved

- **Shuffle** starts playlist playback immediately.
- Playlist track selection and deletion live in the **⋯** menu.
- The close button in the Add music dialog now highlights on hover.
- English and Russian README files include the Shard logo and language navigation.

### Download

**Windows 10 (2004+) / Windows 11, x64:** `Shard-0.1.2-Setup-x64.exe`.

The installer includes the download tools and runtimes. Administrator rights and a separate Rust or Node.js installation are not required. Updating or uninstalling preserves your music and settings.

Spotify provides metadata; audio comes from YouTube. Private playlists and unavailable videos cannot be imported. Individual failures appear in the download report, and completed tracks are retained.

---

## Русский

Shard — лёгкий нативный музыкальный плеер для Windows, написанный на Rust.

### Новое

- Импорт треков, альбомов и открытых плейлистов Spotify. Shard читает публичные метаданные, находит треки на YouTube и скачивает MP3 с сохранением названий, исполнителей и порядка.
- При ошибке загрузки выбранного видео приложение пробует следующий подходящий результат YouTube.
- Переключение между русским и английским интерфейсом без перезапуска.
- Категории **«Настройки → Кастомизация»** для тем и фонов и **«Настройки → Язык»** для выбора языка.
- Редактирование названия плейлиста прямо в окне карандаша. Пустое поле сохраняет прежнее название.
- Тумблер наследования цвета обложки или своего цвета шапки; компактные ползунки и ввод HEX.
- Редактирование названий треков и исполнителей через меню трека.

### Улучшения

- **«Перемешать»** сразу запускает воспроизведение плейлиста.
- Выбор треков и удаление плейлиста находятся в меню **⋯**.
- Крестик окна добавления музыки подсвечивается при наведении.
- README на английском и русском с логотипом Shard и ссылками переключения языка.

### Скачать

**Windows 10 (2004+) / Windows 11, x64:** `Shard-0.1.2-Setup-x64.exe`.

В установщик включены инструменты загрузки и необходимые библиотеки. Права администратора и отдельная установка Rust или Node.js не нужны. Обновление и удаление приложения сохраняют музыку и настройки.

Spotify используется для метаданных, аудио загружается с YouTube. Закрытые плейлисты и недоступные видео импортировать нельзя. Ошибки отдельных треков отображаются в отчёте, уже скачанные треки сохраняются.
