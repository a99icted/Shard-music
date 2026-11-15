# Shard 0.1.3 — Избранное, Яндекс Музыка и синхронизация по сети

## Русский

В этом обновлении появились «Избранное», импорт Яндекс Музыки и передача музыки между устройствами в локальной сети.

### Избранное

- Системный плейлист с SVG-сердцем, которое подстраивается под цвет темы.
- Сердечки у треков добавляют музыку в «Избранное» и показывают одинаковый статус во всех плейлистах.
- Добавление в обычные плейлисты остаётся в меню **⋯** каждого трека.

### Яндекс Музыка

- Импорт треков, альбомов и открытых плейлистов по ссылке.
- Shard получает названия, исполнителей и порядок треков из метаданных Яндекс Музыки, находит подходящее аудио на YouTube и скачивает его в библиотеку.
- Поддерживаются обычные и новые ссылки на открытые плейлисты.

### Синхронизация по локальной сети

- Включение тумблером во вкладке **«Настройки → Синхронизация»**.
- Подключение по коду: скопируйте его иконкой справа от поля и вставьте в другой клиент. QR-код содержит тот же код и подготовлен для будущего мобильного приложения.
- Передача отдельного трека, плейлиста, «Избранного» или всех треков библиотеки.
- Прогресс передачи на обоих клиентах и справка по знаку **?**.

### Дубликаты и продолжение передачи

- Перед отправкой плейлиста или всей библиотеки открывается окно настроек и подтверждения.
- **«Предупреждать о дубликатах» включено**, **«Игнорировать дубликаты» выключено** по умолчанию.
- При совпадении можно использовать имеющийся трек или передать дополнительную копию — отдельно для каждого трека либо сразу для всех совпадений.
- Проверяются содержимое файла, ссылка на источник и метаданные, включая совпадения внутри отправляемой коллекции.
- Повторная синхронизация пополненного плейлиста добавляет новые треки, сохраняя уже переданные.
- После обрыва повторите отправку: она продолжится с сохранённого места, в том числе после перезапуска приложения. Потеря финального подтверждения не создаёт повторных копий.

### Установка и обновление

**Windows 10 (2004+) / Windows 11, x64:** скачайте `Shard-0.1.3-Setup-x64.exe`.

Установщик включает инструменты загрузки и необходимые библиотеки. Права администратора и отдельная установка Rust или Node.js не нужны. Обновление сохраняет музыку и настройки. Перед установкой завершите Shard через меню трея.

Для синхронизации обновите оба клиента до этой версии. Передача запускается вручную в доверенной локальной сети; обложки, фоны, настройки и удаления не переносятся. QR-сканер для мобильного приложения в эту версию не входит.

Spotify и Яндекс Музыка предоставляют метаданные; аудио скачивается с YouTube. Доступность импорта зависит от открытых метаданных и доступных видео.

---

## English

### Favorites

- A built-in Favorites playlist with a theme-colored SVG heart.
- Track hearts share persistent favorite status across every playlist.
- Adding to regular playlists remains in each track's **⋯** menu.

### Yandex Music imports

- Import tracks, albums and public playlists by link, including both playlist URL formats.
- Shard reads public metadata, matches audio on YouTube, and downloads it while preserving titles, artists and track order.

### LAN synchronization

- Enable the switch in **Settings → Synchronization** on both devices and pair them using a connection code.
- Copy the code with the icon next to its read-only field. The QR contains the same payload for a future mobile client.
- Send a track, playlist, Favorites or the entire library, with progress on both clients.
- Help dialogs explain pairing and duplicate options.

### Duplicate handling and resume

- Playlist and whole-library transfers open an options and confirmation dialog.
- **Warn about duplicates defaults to on; Ignore duplicates defaults to off.**
- Reuse an existing track or send another copy, individually or for all remaining matches.
- Detect matches by file contents, source URL and metadata, including duplicates within the selection.
- Sending an updated playlist again adds its new tracks and reuses previously synchronized entries.
- Retry an interrupted transfer to resume from the saved position, even after restarting the app. A lost final acknowledgement does not create extra copies.

### Download and update

**Windows 10 (2004+) / Windows 11, x64:** `Shard-0.1.3-Setup-x64.exe`.

Download tools and runtimes are included. No administrator rights or separate Rust or Node.js installation are required. Updating preserves music and settings. Quit Shard from the tray before updating.

Update both clients to this version before pairing. Transfers are manual and intended for a trusted LAN. Covers, backgrounds, settings and deletions are not transferred. A mobile QR scanner is not included yet.

Spotify and Yandex Music supply metadata; audio is downloaded from YouTube. Import availability depends on public metadata and accessible videos.
