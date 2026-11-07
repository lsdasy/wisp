# Wisp

Универсальный лаунчер для Windows и macOS в стиле Raycast и PowerToys Run. Нажимаете **Alt + Space**, вводите запрос, Wisp сам понимает, что нужно: открыть программу, найти файл, выполнить команду или посчитать выражение.

Стек: **Rust** (ядро), **Tauri 2** (окно, трей, горячие клавиши), **React + TypeScript** (интерфейс), **SQLite** (история, избранное, настройки).

## Установка

**Windows** — в PowerShell:

```powershell
irm https://raw.githubusercontent.com/lsdasy/wisp/main/install.ps1 | iex
```

**macOS / Linux** — в терминале:

```bash
curl -fsSL https://raw.githubusercontent.com/lsdasy/wisp/main/install.sh | bash
```

Скрипт скачивает установщик из последнего [релиза](https://github.com/lsdasy/wisp/releases), сверяет SHA-256 и запускает его. Можно и вручную: `*-setup.exe` для Windows, `*.dmg` для macOS.

Параметры:

| Windows | macOS / Linux | Что делает |
|---|---|---|
| `-Version v0.2.0` | `--version v0.2.0` | конкретная версия вместо последней |
| `-FromSource` | `--source` | собрать из исходников (нужны Git, Rust, Node.js 18+) |
| `-Silent` | — | тихая установка |

Передать параметры при установке одной строкой (Windows):

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/lsdasy/wisp/main/install.ps1))) -FromSource
```

> Сборки пока не подписаны сертификатом. Windows SmartScreen покажет предупреждение: «Подробнее» → «Выполнить в любом случае». macOS при установке через браузер скажет, что разработчик не проверен: правый клик по Wisp.app → «Открыть». Через `install.sh` такого предупреждения нет.

Удаление: Windows — «Параметры → Приложения → Wisp → Удалить»; macOS — перетащить `Wisp.app` из «Программ» в корзину.

## Что умеет

| Ввод | Результат |
|---|---|
| `vsc`, `tlg` | приложения (нечёткий поиск, по первым буквам слов) |
| `code -- C:\Projects\wisp` | запуск приложения с аргументами (всё после ` -- `) |
| `отчёт`, `main.rs` | файлы и папки из индексируемых папок |
| `.pdf` | все файлы с расширением |
| `sqrt(144) * 3 + 15%` | калькулятор; Enter копирует ответ |
| `выключить`, `lock`, `корзина` | системные команды (опасные требуют второго Enter) |
| `> Get-Process` | выполнить в PowerShell (на macOS — в shell) и показать вывод |
| `gs` | свои команды-алиасы из настроек |
| `dps` | команды плагинов |

Клавиши: **↑ ↓** выбор, **Enter** выполнить, **Ctrl/⌘ + Enter** показать в папке, **Ctrl/⌘ + K** меню действий, **Ctrl/⌘ + D** в избранное, **Ctrl/⌘ + ,** настройки, **Esc** назад/скрыть.

## Разработка

Понадобятся Rust (stable), Node.js 18+, а также:

- **Windows:** Microsoft C++ Build Tools (`winget install Microsoft.VisualStudio.2022.BuildTools`) и WebView2 (в Windows 10/11 обычно уже есть);
- **macOS:** `xcode-select --install`;
- **Linux:** `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev`.

```bash
npm install
npm run tauri dev      # запуск с горячей перезагрузкой интерфейса
cargo test -p wisp-core   # тесты ядра
npm run tauri build    # установщики → target/release/bundle/
```

### Выпуск версии

1. Поднять версию в `package.json`, `src-tauri/Cargo.toml` и `src-tauri/tauri.conf.json`.
2. `git tag v0.1.0 && git push origin v0.1.0`.
3. GitHub Actions (`.github/workflows/release.yml`) соберёт установщики для Windows, macOS (Apple Silicon и Intel) и Linux, создаст релиз и приложит `SHA256SUMS`. После этого начнут работать команды установки.

## Архитектура

```
wisp/
├── crates/wisp-core/      ядро без UI — тестируется обычным cargo test
│   └── src/
│       ├── engine.rs       core: разбор запроса, опрос провайдеров, кэш, выполнение
│       ├── matcher.rs      search: нечёткое сравнение
│       ├── launcher.rs     launcher: индекс приложений, частота запусков
│       ├── filesystem.rs   filesystem: индекс файлов + notify
│       ├── commands.rs     commands: системные команды, алиасы, PowerShell/shell
│       ├── calculator.rs   calculator: рекурсивный спуск, без eval
│       ├── plugins.rs      plugins: манифесты, права, проверка ответов
│       ├── storage.rs      storage: SQLite
│       ├── settings.rs     настройки
│       └── platform/       platform: windows.rs, macos.rs, linux.rs
├── src-tauri/              приложение: окна, трей, hotkeys, invoke-команды
└── src/                    ui: React + TypeScript (окно поиска и окно настроек)
```

Ключевые решения:

- **Ядро не зависит от Tauri.** Всю логику можно тестировать без окна. Если захочется заменить интерфейс, ядро останется прежним.
- **UI передаёт только `id` результата.** Что именно выполнить, Rust берёт из своего кэша. Интерфейс не может подсунуть произвольную команду.
- **Подтверждение опасных операций проверяется в Rust,** а не только в интерфейсе.
- **Устаревшие запросы отменяются.** Каждый поиск получает номер; новый номер прерывает обход индекса файлов у предыдущего.
- **Индекс файлов в памяти, а не Tantivy/FTS5.** Мы ищем по именам, а не по содержимому, и плоский список имён в нижнем регистре быстрее и проще. FTS5 стоит подключать для поиска по содержимому.

## Плагины

Плагин — папка в каталоге данных Wisp (`Настройки → Расширения → Открыть папку плагинов`) с файлом `plugin.json`:

```json
{
  "id": "docker",
  "name": "Docker",
  "version": "0.1.0",
  "permissions": ["exec"],
  "commands": [
    { "keyword": "dps", "title": "Docker: контейнеры", "exec": ["docker", "ps"], "output": "text" }
  ]
}
```

- `exec` — программа и аргументы. Запускается напрямую, без shell. Подстановки: `{args}` (аргументы после ключевого слова), `{query}` (они же одной строкой), `{plugin_dir}`.
- `output: "text"` показывает stdout. `output: "items"` ждёт JSON-массив `[{ "title", "subtitle"?, "copy"?, "url"? }]`, который становится списком результатов.
- Права: `exec` (запуск программ), `open` (открытие ссылок). Новый плагин выключен, каждое право пользователь включает в настройках отдельно. Ответ плагина проверяется: не больше 100 пунктов, ограничения длины, открываются только `http(s)`-ссылки.

Примеры: `examples/plugins/docker` и `examples/plugins/web-search`. Скопируйте папку в каталог плагинов и нажмите «Перечитать».

## Дальше

- [ ] настоящие иконки приложений (извлечение из .exe/.lnk и .icns);
- [ ] история буфера обмена;
- [ ] конвертер валют и единиц;
- [ ] поиск процессов, CPU/RAM;
- [ ] плагины на WebAssembly с песочницей вместо внешних процессов;
- [ ] подпись сборок и автообновление (tauri-plugin-updater).

## Лицензия

MIT
