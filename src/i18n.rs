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
        "Избранное" => "Favorites",
        "Избранное — системный плейлист" => {
            "Favorites is a system playlist"
        }
        "Вставьте код другого устройства" => {
            "Paste the other device’s code"
        }
        "Устройства подключены" => "Devices connected",
        "Передача завершена" => "Transfer complete",
        "Синхронизация" => "Synchronization",
        "Синхронизация по локальной сети" => {
            "Local network synchronization"
        }
        "Подключите два устройства к одной сети. Передавайте выбранные треки и плейлисты через меню ⋯. Музыка сохраняется на принимающем устройстве." => {
            "Connect two devices to the same network. Send selected tracks and playlists through the ⋯ menu. Music is saved on the receiving device."
        }
        "Выключить синхронизацию" => "Turn off synchronization",
        "Включить синхронизацию" => "Turn on synchronization",
        "Это устройство" => "This device",
        "Скопировать код подключения" => "Copy connection code",
        "QR-код содержит тот же код подключения и подготовлен для будущего мобильного клиента. Код меняется при повторном включении синхронизации." => {
            "The QR code contains the same connection code for a future mobile client. Turning synchronization on again generates a new code."
        }
        "Подключить устройство" => "Connect device",
        "Подключено" => "Connected",
        "Ожидание подключения устройства" => {
            "Waiting for a device to connect"
        }
        "Передача запускается вручную. Полученные треки добавляются без удаления музыки на другом устройстве. Для подключения может потребоваться разрешить Shard в брандмауэре Windows для частной сети." => {
            "Transfers start manually. Received tracks are added without deleting music on either device. You may need to allow Shard through Windows Firewall on a private network."
        }
        "Ошибка синхронизации" => "Synchronization error",
        "Получение музыки" => "Receiving music",
        "Отправка музыки" => "Sending music",
        "Подготовка передачи…" => "Preparing transfer…",
        "Синхронизировать плейлист" => "Synchronize playlist",
        "Синхронизировать трек" => "Synchronize track",
        "YouTube, Spotify или Яндекс Музыка: трек, альбом или плейлист" => {
            "YouTube, Spotify or Yandex Music: track, album or playlist"
        }
        "Плейлист сохранится в приложении. Треки Spotify и Яндекс Музыки ищутся на YouTube по названию и исполнителю." => {
            "The playlist is saved in the app. Spotify and Yandex Music tracks are matched on YouTube by title and artist."
        }
        "Создайте плейлист или добавьте его по ссылке YouTube, Spotify или Яндекс Музыки." => {
            "Create a playlist or import one from YouTube, Spotify or Yandex Music."
        }
        "Вставьте ссылку YouTube, Spotify или Яндекс Музыки на трек или открытый плейлист" => {
            "Paste a YouTube, Spotify or Yandex Music track or public playlist link"
        }
        "Вставьте HTTPS-ссылку Яндекс Музыки" => {
            "Paste an HTTPS Yandex Music link"
        }
        "Нужна ссылка music.yandex.ru на трек, альбом или открытый плейлист" => {
            "Use a music.yandex.ru track, album or public playlist link"
        }
        "Некорректная ссылка Яндекс Музыки" => {
            "Invalid Yandex Music link"
        }
        "Не удалось получить открытые метаданные Яндекс Музыки" => {
            "Could not retrieve public Yandex Music metadata"
        }
        "Коллекция Яндекс Музыки закрыта или недоступна" => {
            "Yandex Music collection is private or unavailable"
        }
        "Некорректный ответ Яндекс Музыки" => {
            "Invalid Yandex Music response"
        }
        "Яндекс Музыка не предоставила все треки плейлиста" => {
            "Yandex Music did not provide all playlist tracks"
        }
        "Трек без названия" => "Track has no title",
        "Трек без исполнителя" => "Track has no artist",
        "Яндекс Музыка вернула неполные метаданные трека" => {
            "Yandex Music returned incomplete track metadata"
        }
        "В Яндекс Музыке нет доступных треков" => {
            "No available Yandex Music tracks"
        }
        "Код подключения" => "Connection code",
        "Код подключения скопирован" => "Connection code copied",
        "ПОМОЩЬ" => "HELP",
        "Как работает синхронизация" => "How synchronization works",
        "Передавайте музыку между устройствами в одной локальной сети." => {
            "Transfer music between devices on the same local network."
        }
        "Понятно" => "Got it",
        "Включена" => "On",
        "Выключена" => "Off",
        "Включите синхронизацию" => "Turn on synchronization",
        "На обоих устройствах откройте «Настройки → Синхронизация» и включите тумблер. Устройства должны быть подключены к одной сети Wi-Fi или локальной сети." => {
            "Open Settings → Synchronization on both devices and turn on the switch. Both devices must be connected to the same Wi-Fi or local network."
        }
        "Соедините устройства" => "Connect the devices",
        "Нажмите иконку копирования справа от кода подключения на одном устройстве. Вставьте код в поле на другом устройстве и нажмите «Подключить устройство». Достаточно ввести код на одном из двух клиентов." => {
            "Click the copy icon next to the connection code on one device. Paste it into the field on the other device and click Connect device. You only need to enter the code on one of the two clients."
        }
        "Выберите музыку для передачи" => "Choose music to send",
        "Откройте меню ⋯ у трека или в шапке плейлиста и выберите «Синхронизировать трек» или «Синхронизировать плейлист». Так можно передать и «Избранное». Прогресс передачи отображается на обоих устройствах." => {
            "Open the ⋯ menu for a track or in a playlist header and choose Synchronize track or Synchronize playlist. You can send Favorites too. Transfer progress appears on both devices."
        }
        "Слушайте на другом устройстве" => "Listen on the other device",
        "Аудиофайлы сохранятся в библиотеке получателя вместе с названиями, исполнителями и отметками избранного. При повторной отправке плейлиста добавятся новые треки. Для других совпадений можно выбрать пропуск или дополнительную копию. После обрыва повторите отправку, чтобы продолжить. Удаление музыки на одном устройстве не удаляет её на другом." => {
            "Audio files are saved in the receiving device’s library with titles, artists and favorite marks. Sending a playlist again adds new tracks. For other matches, you can skip them or send another copy. Repeat an interrupted transfer to resume it. Deleting music on one device does not delete it on the other."
        }
        "QR-код предназначен для будущего мобильного приложения. После перезапуска синхронизации код меняется: устройства нужно подключить заново. Обложки, фоны и настройки пока не передаются." => {
            "The QR code is for a future mobile app. Restarting synchronization changes the code, so you need to reconnect the devices. Covers, backgrounds and settings are not transferred yet."
        }
        "Передача отменена" => "Transfer cancelled",
        "Предупреждать о дубликатах" => "Warn about duplicates",
        "Показать найденные совпадения перед передачей." => {
            "Show detected matches before transferring."
        }
        "Игнорировать дубликаты" => "Ignore duplicates",
        "Использовать уже имеющиеся треки, не создавая новые копии." => {
            "Reuse existing tracks without creating new copies."
        }
        "При совпадении вы сможете пропустить трек или передать копию. Решение можно применить ко всем совпадениям." => {
            "For each match, you can skip the track or send a copy. You can apply your choice to all matches."
        }
        "Совпадения будут пропущены без предупреждения. В плейлист добавятся уже имеющиеся треки." => {
            "Matches will be skipped without a warning. Existing tracks will be added to the playlist."
        }
        "Предупреждения выключены: найденные дубликаты будут переданы как новые копии." => {
            "Warnings are off: detected duplicates will be transferred as new copies."
        }
        "Уже синхронизированные треки повторно не копируются. После обрыва повторите отправку — передача продолжится с сохранённого места." => {
            "Previously synchronized tracks are reused. Repeat an interrupted transfer to resume from the saved position."
        }
        "Начать синхронизацию" => "Start synchronization",
        "Если включено, передача остановится перед отправкой совпадений и спросит, что делать. Можно решить для каждого трека отдельно или включить «Применить ко всем найденным дубликатам»." => {
            "When enabled, the transfer pauses before sending matches and asks what to do. Decide for each track or enable Apply to all detected duplicates."
        }
        "Если включено, дубликаты пропускаются: в плейлисте используется трек из библиотеки получателя. Если выключено, можно передать дополнительную копию. При включённых предупреждениях вы подтверждаете решение; без предупреждений оно применяется автоматически." => {
            "When enabled, duplicates are skipped: the playlist uses a track from the receiver's library. When disabled, you can send an additional copy. With warnings on, you confirm the choice; without warnings it is applied automatically."
        }
        "По умолчанию предупреждения включены, а игнорирование выключено. Само совпадение не запрещает отправку: вы всегда можете выбрать «Передать копию»." => {
            "By default, warnings are on and ignoring is off. A match does not block sending: you can always choose Send a copy."
        }
        "Совпадения проверяются по содержимому файла, ссылке на источник и метаданным. Уже синхронизированные треки обновляемого плейлиста переиспользуются автоматически. Обрыв связи не стирает полученные данные — повторная отправка продолжится с места остановки." => {
            "Matches are checked by file contents, source link and metadata. Previously synchronized tracks in an updated playlist are reused automatically. A disconnection preserves received data, so another attempt resumes from the saved position."
        }
        "Найден файл с тем же содержимым." => {
            "A file with identical contents was found."
        }
        "Найден трек с той же ссылкой на источник." => {
            "A track with the same source link was found."
        }
        "Совпадают название, исполнитель и длительность. Аудиофайлы могут отличаться." => {
            "Title, artist and duration match. The audio files may differ."
        }
        "Применить ко всем найденным дубликатам" => {
            "Apply to all detected duplicates"
        }
        "Выключите, чтобы решить для каждого трека отдельно." => {
            "Turn off to decide for each track separately."
        }
        "Передать копии" => "Send copies",
        "Передать копию" => "Send a copy",
        "Пропустить дубликаты" => "Skip duplicates",
        "Пропустить дубликат" => "Skip duplicate",
        "Отменить передачу" => "Cancel transfer",
        "Синхронизировать все треки" => "Synchronize all tracks",
        "ПЕРЕДАЧА МУЗЫКИ" => "MUSIC TRANSFER",
        "Настройки синхронизации" => "Synchronization options",
        "Как обрабатывать дубликаты" => "How to handle duplicates",
        "Выберите, что делать с треками, которые уже есть на другом устройстве." => {
            "Choose what to do with tracks that already exist on the other device."
        }
        "Найдены дубликаты" => "Duplicates found",
        "Выберите: использовать имеющийся трек или передать ещё одну копию." => {
            "Choose whether to use an existing track or send another copy."
        }
        _ => text,
    }
}
#[macro_export]
macro_rules! localized_format {
    ($ru:literal, $en:literal $($args:tt)*) => {
        if $crate::i18n::is_english() { format!($en $($args)*) } else { format!($ru $($args)*) }
    };
}
