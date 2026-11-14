//! Application language shared by the UI and background workers.
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    #[default]
    Russian,
    English,
}
static ENGLISH: AtomicBool = AtomicBool::new(false);
pub fn set_language(language: Language) {
    ENGLISH.store(language == Language::English, Ordering::Relaxed);
}
pub fn is_english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}
pub fn tr(text: &str) -> &str {
    if !is_english() {
        return text;
    }
    match text {
        "Поиск по названию или исполнителю" => {
            "Search by title or artist"
        }
        "Исполнитель" => "Artist",
        "Ваша библиотека" => "Your library",
        "Название плейлиста" => "Playlist name",
        "Название трека" => "Track title",
        "Например, Вечерний маршрут" => "For example, Evening drive",
        "Свой цвет: #34465E" => "Custom color: #34465E",
        "Сохраняем…" => "Saving…",
        "Получаем список треков…" => "Getting track list…",
        "Создаём плейлист…" => "Creating playlist…",
        "Плейлист создан. Добавьте в него треки из библиотеки" => {
            "Playlist created. Add tracks from your library"
        }
        "Добавляем локальные файлы…" => "Importing local files…",
        "Добавить музыку в Shard" => "Add music to Shard",
        "Аудиофайлы" => "Audio files",
        "Не удалось открыть окно выбора файла" => {
            "Could not open file picker"
        }
        "Добавляем трек в плейлист…" => "Adding track to playlist…",
        "Трек добавлен в плейлист" => "Track added to playlist",
        "Этот трек уже есть в плейлисте" => {
            "This track is already in the playlist"
        }
        "Все треки" => "All tracks",
        "Готовим обложку…" => "Preparing cover…",
        "Обложка плейлиста" => "Playlist cover",
        "Изображения" => "Images",
        "Не удалось открыть выбор изображения" => {
            "Could not open image picker"
        }
        "Сохраняем цвет плейлиста…" => "Saving playlist color…",
        "Этот трек уже есть в очереди" => {
            "This track is already in the queue"
        }
        "Трек добавлен в очередь" => "Track added to queue",
        "Убираем трек из плейлиста…" => "Removing track from playlist…",
        "Трек убран из плейлиста. Он доступен в общей библиотеке" => {
            "Track removed from playlist. It is still available in your library"
        }
        "Удаляем трек из хранилища…" => "Deleting track from storage…",
        "Удаляем плейлист…" => "Deleting playlist…",
        "Очередь воспроизведения" => "Playback queue",
        "ВАША МУЗЫКА" => "YOUR MUSIC",
        "⌂ Главная" => "⌂ Home",
        "ПЛЕЙЛИСТЫ" => "PLAYLISTS",
        "⚙  Настройки" => "⚙  Settings",
        "●  Локальная коллекция" => "●  Local collection",
        "Ваша музыка. На вашем устройстве." => {
            "Your music. On your device."
        }
        "УПРАВЛЕНИЕ ПЛЕЙЛИСТОМ" => "MANAGE PLAYLIST",
        "Действия с плейлистом" => "Playlist actions",
        "Переименовать плейлист" => "Rename playlist",
        "Введите новое название плейлиста." => {
            "Enter a new playlist name."
        }
        "УПРАВЛЕНИЕ ТРЕКОМ" => "MANAGE TRACK",
        "Изменить трек" => "Edit track",
        "Измените название и исполнителя в библиотеке." => {
            "Edit the title and artist in your library."
        }
        "Удалить плейлист?" => "Delete playlist?",
        "ОФОРМЛЕНИЕ ПОДБОРКИ" => "PLAYLIST APPEARANCE",
        "Оформление плейлиста" => "Playlist appearance",
        "Выберите обложку — цвет шапки подстроится под неё. Можно задать свой оттенок." => {
            "Choose a cover to match the header color to it, or set a custom color."
        }
        "ПОПОЛНИТЬ КОЛЛЕКЦИЮ" => "GROW YOUR COLLECTION",
        "Добавить музыку" => "Add music",
        "НОВАЯ ПОДБОРКА" => "NEW COLLECTION",
        "Создать плейлист" => "Create playlist",
        "Дайте название своей следующей музыкальной истории." => {
            "Give your next musical story a name."
        }
        "ДОБАВИТЬ В ПОДБОРКУ" => "ADD TO PLAYLIST",
        "Выберите плейлист" => "Choose a playlist",
        "ИЗ ВАШЕЙ БИБЛИОТЕКИ" => "FROM YOUR LIBRARY",
        "Добавить треки" => "Add tracks",
        "Нажмите на трек, чтобы добавить его в этот плейлист." => {
            "Click a track to add it to this playlist."
        }
        "Действия с треком" => "Track actions",
        "УДАЛЕНИЕ ИЗ ХРАНИЛИЩА" => "DELETE FROM STORAGE",
        "Удалить трек?" => "Delete track?",
        "+  Треки" => "+  Tracks",
        "Удалить плейлист" => "Delete playlist",
        "Сохранить" => "Save",
        "Отмена" => "Cancel",
        "Выбрать обложку…" => "Choose cover…",
        "Применить свой цвет" => "Apply custom color",
        "Подбирать цвет по обложке" => "Match cover color",
        "Убрать обложку" => "Remove cover",
        "Убираем обложку…" => "Removing cover…",
        "↗   Выбрать аудиофайлы" => "↗   Choose audio files",
        "Скачать и добавить" => "Download and add",
        "Плейлист сохранится в приложении. Треки Spotify ищутся на YouTube по названию и исполнителю." => {
            "The playlist is saved in the app. Spotify tracks are matched on YouTube by title and artist."
        }
        "Пока нет плейлистов. Создайте первый кнопкой ниже." => {
            "No playlists yet. Create your first one below."
        }
        "+  Новый плейлист" => "+  New playlist",
        "Сначала добавьте музыку в общую библиотеку." => {
            "Add music to your library first."
        }
        "Изменить название и исполнителя" => "Edit title and artist",
        "≡  Добавить в очередь" => "≡  Add to queue",
        "+  Добавить в плейлист" => "+  Add to playlist",
        "−  Убрать из этого плейлиста" => "−  Remove from this playlist",
        "Удалить из хранилища…" => "Delete from storage…",
        "Удалить файл и трек" => "Delete file and track",
        "Остановить загрузку" => "Stop download",
        "ВАШ ПЛЕЙЛИСТ" => "YOUR PLAYLIST",
        "МУЗЫКАЛЬНАЯ БИБЛИОТЕКА" => "MUSIC LIBRARY",
        "Нажмите на обложку, чтобы изменить оформление" => {
            "Click the cover to change appearance"
        }
        "↓ Загрузка" => "↓ Download",
        "Обработка" => "Processing",
        "Останавливаем…" => "Stopping…",
        "Остановить" => "Stop",
        "Не удалось выполнить действие" => "Could not complete action",
        "Свернуть" => "Collapse",
        "Подробнее" => "Details",
        "Очередь" => "Queue",
        "СЕЙЧАС ИГРАЕТ" => "NOW PLAYING",
        "Выберите трек или нажмите «Слушать»" => {
            "Choose a track or press Play"
        }
        "В очереди пока пусто" => "The queue is empty",
        "Выберите подборку или добавьте трек через меню ⋯" => {
            "Choose a collection or add a track using the ⋯ menu"
        }
        "⤨  Перемешивание включено" => "⤨  Shuffle on",
        "В порядке выбранной подборки" => "In collection order",
        "Музыка всегда рядом" => "Your music, always with you",
        "Выберите трек в библиотеке" => "Choose a track in your library",
        "↻ Выкл" => "↻ Off",
        "↻ Всё" => "↻ All",
        "↻ Трек" => "↻ Track",
        "≡ Очередь" => "≡ Queue",
        "Ⅱ  Пауза" => "Ⅱ  Pause",
        "▶  Слушать" => "▶  Play",
        "⤨  Перемешать" => "⤨  Shuffle",
        "+  Музыка" => "+  Music",
        "+  Добавить музыку" => "+  Add music",
        "Треки" => "Tracks",
        "В порядке плейлиста" => "Playlist order",
        "Недавно добавленные" => "Recently added",
        "Ничего не найдено" => "No results",
        "Здесь будет ваша музыка" => "Your music will appear here",
        "Попробуйте другое название или исполнителя" => {
            "Try another title or artist"
        }
        "Добавьте музыку или выберите треки из библиотеки" => {
            "Add music or choose tracks from your library"
        }
        "НАЗВАНИЕ / ИСПОЛНИТЕЛЬ" => "TITLE / ARTIST",
        "ВРЕМЯ" => "TIME",
        "Неизвестный исполнитель" => "Unknown artist",
        "Дождитесь завершения текущей операции" => {
            "Wait for the current operation to finish"
        }
        "Готовим фон…" => "Preparing background…",
        "Фон Shard" => "Shard background",
        "Применяем размытие…" => "Applying blur…",
        "Фон приложения" => "App background",
        "Фон плейлиста" => "Playlist background",
        "PNG, JPEG или WebP, до 25 МБ. Фон используется везде, где не задан фон плейлиста." => {
            "PNG, JPEG or WebP, up to 25 MB. Used wherever a playlist background is not set."
        }
        "Собственный фон заменяет общий только в этом плейлисте." => {
            "A custom background replaces the app background for this playlist."
        }
        "Выбрать изображение…" => "Choose image…",
        "Размытие" => "Blur",
        "Перетащите ползунок — размытие применится после отпускания." => {
            "Drag the slider. Blur is applied when you release it."
        }
        "Убрать фон" => "Remove background",
        "Использовать общий фон" => "Use app background",
        "Убираем фон…" => "Removing background…",
        "Сейчас используется фон выбранной темы." => {
            "Currently using the selected theme background."
        }
        "Сейчас используется общий фон приложения." => {
            "Currently using the app background."
        }
        "Настройки" => "Settings",
        "Тема приложения" => "App theme",
        "Настройки сохраняются автоматически. Фон плейлиста меняется кнопкой карандаша в его шапке." => {
            "Settings are saved automatically. Change a playlist background using the pencil in its header."
        }
        "Ваша музыка" => "Your music",
        "+ Музыка" => "+ Music",
        "В этом месяце" => "This month",
        "В этом году" => "This year",
        "Ваши плейлисты" => "Your playlists",
        "+ Создать" => "+ Create",
        "Создайте плейлист или добавьте его по ссылке YouTube или Spotify." => {
            "Create a playlist or import one from YouTube or Spotify."
        }
        "Топ-3 за всё время" => "All-time top 3",
        "Здесь появятся треки, которые вы слушаете чаще всего." => {
            "Your most played tracks will appear here."
        }
        "Статистика считается с этой версии по времени воспроизведения. Пауза и перемотка не учитываются." => {
            "Statistics track listening time starting with this version. Pauses and seeking are excluded."
        }
        "Загрузка остановлена. " => "Download stopped. ",
        "YouTube не вернул результаты поиска" => {
            "YouTube returned no search results"
        }
        "На YouTube не найдено подходящего совпадения" => {
            "No suitable match found on YouTube"
        }
        "Не удалось прочитать метаданные загрузки" => {
            "Could not read download metadata"
        }
        "Не удалось прочитать список треков YouTube" => {
            "Could not read YouTube track list"
        }
        "В плейлисте нет доступных для просмотра треков" => {
            "The playlist has no available tracks"
        }
        "Плейлист YouTube" => "YouTube playlist",
        "Загрузчик завершился с ошибкой" => "Downloader failed",
        "Загрузка остановлена" => "Download stopped",
        "Повтор выключен" => "Repeat off",
        "Повтор всей очереди" => "Repeat queue",
        "Повтор одного трека" => "Repeat track",
        "Лес" => "Forest",
        "Оникс" => "Onyx",
        "Пикми" => "Pick Me",
        "Космос" => "Cosmic",
        "Продолжить" => "Resume",
        "Пауза" => "Pause",
        "Предыдущий трек" => "Previous track",
        "Следующий трек" => "Next track",
        "Некорректный путь аудиофайла в базе" => {
            "Invalid audio file path in database"
        }
        "Введите название трека" => "Enter a track title",
        "Не больше 500 символов в каждом поле" => {
            "Use at most 500 characters per field"
        }
        "Не удалось подготовить аудиофайл к удалению" => {
            "Could not prepare audio file for deletion"
        }
        "Не удалось прочитать аудиофайл" => "Could not read audio file",
        "Не удалось сохранить аудиофайл" => "Could not save audio file",
        "Трек YouTube" => "YouTube track",
        "YouTube не предоставил ссылку на этот трек (видео удалено или скрыто)" => {
            "YouTube did not provide a track link (the video is deleted or private)"
        }
        "Вставьте ссылку YouTube или Spotify на трек или открытый плейлист" => {
            "Paste a YouTube or Spotify track or public playlist link"
        }
        "Вставьте полную ссылку YouTube, начиная с https://" => {
            "Paste a full YouTube link starting with https://"
        }
        "Нужна HTTPS-ссылка YouTube" => "Use an HTTPS YouTube link",
        "Поддерживаются ссылки youtube.com и youtu.be" => {
            "Links from youtube.com and youtu.be are supported"
        }
        "Укажите ссылку на трек, видео или плейлист" => {
            "Enter a track, video or playlist link"
        }
        "Введите название плейлиста" => "Enter a playlist name",
        "Название должно содержать не больше 100 символов" => {
            "Use at most 100 characters for the name"
        }
        "Не удалось определить папку данных приложения" => {
            "Could not find app data directory"
        }
        "Вставьте HTTPS-ссылку Spotify" => "Paste an HTTPS Spotify link",
        "Нужна ссылка open.spotify.com на трек, альбом или открытый плейлист" => {
            "Use an open.spotify.com track, album or public playlist link"
        }
        "Некорректная ссылка Spotify" => "Invalid Spotify link",
        "Не удалось получить открытые метаданные Spotify" => {
            "Could not retrieve public Spotify metadata"
        }
        "Не удалось прочитать ответ Spotify" => {
            "Could not read Spotify response"
        }
        "Spotify не предоставил открытый список треков" => {
            "Spotify did not provide a public track list"
        }
        "Некорректный ответ Spotify" => "Invalid Spotify response",
        "Некорректные метаданные Spotify" => "Invalid Spotify metadata",
        "Spotify вернул другую коллекцию" => {
            "Spotify returned a different collection"
        }
        "Коллекция Spotify закрыта или недоступна" => {
            "Spotify collection is private or unavailable"
        }
        "Spotify вернул трек без названия" => {
            "Spotify returned a track without a title"
        }
        "Spotify вернул трек без исполнителя" => {
            "Spotify returned a track without an artist"
        }
        "В Spotify нет доступных треков" => "No available Spotify tracks",
        "Трек Spotify не найден" => "Spotify track not found",
        "Трек не найден" => "Track not found",
        "Плейлист не найден" => "Playlist not found",
        "Выберите изображение до 25 МБ" => "Choose an image up to 25 MB",
        "Изображение должно содержать не больше 40 миллионов пикселей" => {
            "Image must contain at most 40 million pixels"
        }
        "Не удалось открыть изображение. Поддерживаются PNG, JPEG и WebP" => {
            "Could not open image. PNG, JPEG and WebP are supported"
        }
        "Цвет должен быть в формате #RRGGBB" => {
            "Use a color in #RRGGBB format"
        }
        "Введите цвет в формате #RRGGBB, например #34465E" => {
            "Enter a color in #RRGGBB format, for example #34465E"
        }
        "Поддерживаются PNG, JPEG и WebP" => "PNG, JPEG and WebP are supported",
        "Некорректная область фона" => "Invalid background scope",
        "Размытие должно быть от 0 до 40" => "Blur must be between 0 and 40",
        "Сначала выберите фон" => "Choose a background first",
        "Не найден компонент загрузки yt-dlp. Запустите scripts/setup-youtube.ps1 из папки Shard или положите yt-dlp рядом с приложением в папку tools" => {
            "yt-dlp not found. Run scripts/setup-youtube.ps1 from the Shard directory or place yt-dlp in the tools directory next to the app"
        }
        "Не найден компонент обработки аудио FFmpeg. Запустите scripts/setup-youtube.ps1 из папки Shard или положите FFmpeg в папку tools" => {
            "FFmpeg not found. Run scripts/setup-youtube.ps1 from the Shard directory or place FFmpeg in the tools directory"
        }
        "Аудиоустройство недоступно" => "Audio device unavailable",
        "Трек отсутствует в очереди" => "Track is not in the queue",
        "Аудиофайл не найден" => "Audio file not found",
        "Не удалось декодировать аудио" => "Could not decode audio",
        "Аудиоплеер недоступен" => "Audio player unavailable",
        "Аудиоплеер не ответил на удаление трека" => {
            "Audio player did not respond to track removal"
        }
        "Редактировать плейлист" => "Edit playlist",
        "Название, обложка, цвет шапки и фон плейлиста." => {
            "Playlist name, cover, header color and background."
        }
        "Цвет шапки" => "Header color",
        "Свой цвет" => "Custom color",
        "Цвет обложки" => "Cover color",
        "Выберите оттенок из палитры или введите HEX." => {
            "Choose a shade from the palette or enter a HEX color."
        }
        "Кастомизация" => "Customization",
        "Язык" => "Language",
        "Язык приложения" => "App language",
        "Оттенок" => "Hue",
        "Насыщенность" => "Saturation",
        "Яркость" => "Lightness",
        _ => text,
    }
}
#[macro_export]
macro_rules! localized_format {
    ($ru:literal, $en:literal $($args:tt)*) => {
        if $crate::i18n::is_english() { format!($en $($args)*) } else { format!($ru $($args)*) }
    };
}
