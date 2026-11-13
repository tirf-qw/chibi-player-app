# Chibi Player — лёгкое приложение (Windows)

Прозрачное окно без рамки поверх всех окон, как Bongo Cat: видна только летающая голова,
по наведению открывается плеер Spotify. Клики мимо головы и панели проходят сквозь окно.
Работает на системном WebView2 (движок Edge), поэтому в приложение не вшит свой Chromium.

## Самый простой способ получить .exe (без установки Rust)
1. Создай аккаунт на github.com и новый репозиторий (Private подойдёт).
2. Загрузи в него все файлы из этой папки (Add file -> Upload files), включая скрытую папку `.github`.
3. Вкладка **Actions** -> **Сборка Chibi Player (Windows)** -> **Run workflow**.
4. Через ~5-10 минут открой завершённый запуск -> внизу **Artifacts** -> скачай `ChibiPlayer-windows`.
   Внутри `chibi-player.exe`, это и есть приложение.

## Spotify
1. На developer.spotify.com/dashboard создай приложение, в **Redirect URI** впиши ровно:
   `http://127.0.0.1:8888/callback`
   В разделе API отметь **Web API** и **Web Playback SDK**.
2. Запусти `chibi-player.exe`, наведи на голову -> **Настройки**, вставь Client ID, нажми «Войти через Spotify».
   Откроется браузер, войди и разреши доступ, потом вернись в плеер.
3. Закрыть плеер: Настройки -> «Закрыть плеер» (иконки в панели задач у него нет).

## Сборка у себя
Нужны Node.js, Rust (rustup.rs) и «Build Tools for Visual Studio» (C++). Потом:
`npm install` и `npx tauri build --no-bundle`. Готовый файл: `src-tauri/target/release/chibi-player.exe`.

## Своя голова
В Настройках -> «Загрузить свою» (PNG с прозрачным фоном).
