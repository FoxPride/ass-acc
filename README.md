# ass-acc

A Rust CLI application for parsing financial statements (PDF, HTML, images via OCR)
and preparing CSVs for upload to FireFly-III.

## Features

- Parsing sources:
  - `PDF` (TTB)
  - `HTML` (Bybit)
  - `JPG` from the `Images` folder (TrueMoney, via OCR)
- Applies transaction rename rules (`[[rules]]`) to category/description
- Exports the result to a `;`-delimited CSV with the columns:
  `Category;Description;Date Time;Amount`
- Uploads prepared CSVs to the FireFly-III Auto Upload endpoint, skipping files
  that haven't been edited since they were parsed

## Requirements

- Rust toolchain
- Configuration file: `Settings/config.toml`
- For OCR (TrueMoney): two RTEN models (detector + recognizer)

## Build and run

### Build

```bash
# Clone the repository
git clone https://github.com/FoxPride/ass-acc.git

# Enter the project directory
cd ass-acc

# Build the project (a release build is recommended because of OCR)
cargo build --release
```

### Main commands

Parse the input files:

```bash
cargo run -- parse
```

Upload the CSVs to FireFly-III (by host address):

```bash
cargo run -- upload <address>
```

Example:

```bash
cargo run -- upload 192.168.1.50
```

Clear the input/output folders:

```bash
cargo run -- clear
```

## Data flow

1. `parse` reads `Settings/config.toml`
2. Scans `input_folder`:
   - `*.pdf` -> TTB parser
   - `*.html` -> Bybit parser
   - the `Images` folder -> TrueMoney OCR parser
3. Applies `[[rules]]` to each transaction
4. Writes the CSV to `output_folder`
5. Writes `parsed_manifest` to `output_folder`, recording the modification time
   of each generated CSV
6. Updates `last_parsed_datetime` in the config

The `upload` command sends the CSVs from `output_folder` to:

`http://<address>:<firefly_port>/autoupload?secret=<client_secret>`

with an `Authorization: Bearer <access_token>` header.

### Editing before upload

`upload` only sends CSVs that have been edited since the last `parse`: a CSV
whose modification time still matches its `parsed_manifest` entry is treated as
unmodified and skipped. If nothing is left to upload, it prints
`Nothing to upload.` and exits successfully.

## Configuration (`Settings/config.toml`)

All fields actually used by the application are described below.

### Base fields

- `input_folder` - folder with the input files for `parse`
- `output_folder` - folder where the CSVs are written (`TTB.csv`, `Bybit.csv`, `TrueMoney.csv`)
- `last_parsed_datetime` - "skip older transactions" filter. Strict format `%d-%m-%Y %H:%M`
- `ocr_models` - paths to the two RTEN models (a `[ocr_models]` table):
  - `detection` - text detection model
  - `recognition` - text recognition model
- `ttb_channels` - list of channel markers for the TTB PDF
- `access_token` - FireFly-III bearer token
- `client_secret` - secret for the auto upload endpoint
- `firefly_port` - port of the FireFly-III server (default `8081`)

### Rename rules `[[rules]]`

- `regex` - required regular expression matched against the original `description`
- `category` - required new category
- `description` - optional replacement description
- `amount` - optional exact amount filter (string comparison)

Rules are applied top to bottom; the first matching rule wins.

### Bybit selectors (`[bybit_selectors]`)

- CSS selectors used to read the columns from the Bybit HTML table
- Only rows with status `Successful` are processed

### TrueMoney OCR

The TrueMoney parser runs OCR over the `Images` folder. It uses the OCR engine's
text-detection bounding boxes to locate each transaction cell (date, description,
time, amount) and recognizes them individually, so no manual region tuning is
required.

## `clear` command behavior

- In `input_folder`, removes all files
  - The `Images` folder itself is kept, but its contents are cleared
- In `output_folder`, removes everything except `.json` files (upload templates)
