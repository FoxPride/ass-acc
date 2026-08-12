# ass-acc

CLI-приложение на Rust для парсинга финансовых выписок (PDF, HTML, изображения через OCR)
и подготовки CSV для загрузки в FireFly-III

## Возможности

- Парсинг источников:
  - `PDF` (TTB)
  - `HTML` (Bybit)
  - `JPG` из папки `Images` (TrueMoney OCR)
- Применение правил переименования транзакций (`[[rules]]`) к категории/описанию
- Экспорт результата в CSV разделенного `;` с колонками:
  `Category;Description;Date Time;Amount`
- Загрузка подготовленных CSV в FireFly-III Auto Upload endpoint

## Требования

- Rust toolchain
- Файл конфигурации: `Settings/config.toml`
- Для OCR (TrueMoney): две RTEN-модели (detector + recognizer)

## Сборка и запуск

### Сборка

```bash
# Откройте терминал (Command Prompt или PowerShell для Windows, Terminal для macOS или Linux)

# Убедитесь, что Git установлен
# Посетите https://git-scm.com чтобы скачать и установить Git, если ещё не установлен

# Клонируйте репозиторий 
git clone https://github.com/FoxPride/ass-acc.git

# Перейдите в директорию проекта
cd ass-acc

# Сборка проекта (рекомендуется использовать release сборку из-за OCR)
cargo build --release
```

### Основные команды

Парсинг входных файлов:

```bash
cargo run -- parse
```

Загрузка CSV в FireFly-III (по адресу хоста):

```bash
cargo run -- upload <address>
```

Пример:

```bash
cargo run -- upload 192.168.1.50
```

Очистка входных/выходных папок:

```bash
cargo run -- clear
```

## Как работает поток данных

1. `parse` читает `Settings/config.toml`
2. Сканирует `input_folder`:
   - `*.pdf` -> парсер TTB
   - `*.html` -> парсер Bybit
   - папка `Images` -> OCR-парсер TrueMoney
3. Для каждой транзакции применяет `[[rules]]`
4. Пишет CSV в `output_folder`
5. Обновляет `last_parsed_datetime` в конфиге

Команда `upload` отправляет CSV из `output_folder` на:

`http://<address>:81/autoupload?secret=<client_secret>`

с `Authorization: Bearer <access_token>`

## Конфигурация (`Settings/config.toml`)

Ниже описаны все поля, которые реально используются приложением.

### Базовые поля

- `input_folder` - папка с входными файлами для `parse`
- `output_folder` - папка, куда пишутся CSV (`TTB.csv`, `Bybit.csv`, `TrueMoney.csv`)
- `last_parsed_datetime` - фильтр "не брать старые операции". Формат строго `%d-%m-%Y %H:%M`
- `ocr_models` - массив из **двух** путей к RTEN-моделям в порядке:
  1) detection
  2) recognition
- `ttb_channels` - список маркеров каналов для PDF TTB
- `access_token` - Bearer token FireFly-III
- `client_secret` - секрет для auto upload endpoint

### Правила переименования `[[rules]]`

- `regex` - обязательное регулярное выражение для поиска по исходному `description`
- `category` - обязательная новая категория
- `description` - опциональная замена описания
- `amount` - опциональный точный фильтр суммы (строковое сравнение)

Применение идет сверху вниз, используется первое совпавшее правило

### Селекторы Bybit (`[bybit_selectors]`)

- CSS-селекторы для чтения колонок из HTML-таблицы Bybit
- Обрабатываются только строки со статусом `Successful`

### Настройки TrueMoney OCR (`[truemoney_config]`)

`region_config` управляет базовыми порогами и геометрией поиска блоков транзакции для OCR

Назначение параметров:

- `region_x`, `region_width` - горизонтальная область, где идет поиск
- `left_bound_start`, `right_bound_start` - стартовые границы сканирования
- `current_region_skip` - шаг поиска в текущем состоянии OCR-пайплайна
- `next_region_skip` - шаг перехода к следующему блоку
- `empty_column_threshold` - порог пустой колонки для определения границ региона

## Поведение команды `clear`

- В `input_folder` удаляет все файлы
  - Папку `Images` не удаляет, но очищает ее содержимое
- В `output_folder` удаляет всё, кроме `.json` файлов (шаблоны для upload)
