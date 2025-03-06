# Image Date Fixer

## Overview

Image Date Fixer is a script that extracts possible timestamp information from filenames and sets the EXIF and modified times accordingly. It ensures that existing EXIF times are never overwritten and corrects dates that are too far in the future by adjusting them to the present.

## Features

- Extracts date information from filenames of various formats, including:
  - Android-style (`IMG_YYYYMMDD_HHMMSS.jpg`)
  - WhatsApp-style (`IMG-YYYYMMDD-WAXXXX.jpg`)
  - Standard date-prefixed (`YYYY-MM-DD_HHMMSS.jpg`)
  - UUID timestamp-prefixed filenames
  - Screenshot-style (`Screenshot_YYYYMMDD-HHMMSS.jpg`)
- Uses EXIF metadata if available
- Corrects file modification dates
- Supports recursive processing of directories
- Can exclude specific directories from processing
- Dry-run mode for testing without modifying files

## Requirements

- Python 3.x
- `exiftool` (must be installed and available in `PATH`)
- The dependencies in `requirements.txt` must be installed

### Setup the tool

```sh
# Clone the repository
git clone <repo-url>
cd image-date-fixer
# Install exiftool
sudo pacman -S perl-image-exiftool  # Arch Linux
# Setup a virtual environment
python -m venv venv
source venv/bin/activate
# Install dependencies
pip install -r requirements.txt
# Now you can run the script
python image_date_fixer.py --help
```

## Usage

### Basic Commands

#### Process a Single File

```sh
python image_date_fixer.py --file /path/to/image.jpg
```

#### Process a Directory Recursively

```sh
python image_date_fixer.py --directory /path/to/images
```

#### Exclude Specific Directories

```sh
python image_date_fixer.py --directory /path/to/images --exclude-dirs exclude_folder1 exclude_folder2
```

#### Fix Future Dates (e.g., if file timestamps are 30+ days in the future)

This happens in addition to the normal processing.

```sh
python image_date_fixer.py --directory /path/to/images --fix-future-dates 30
```

#### Dry Run Mode (No File Modifications)

```sh
python image_date_fixer.py --directory /path/to/images --dry-run
```

### Logging Levels

Set logging verbosity using:

```sh
python image_date_fixer.py --directory /path/to/images --log-level DEBUG
```

Available levels: `DEBUG`, `INFO`, `WARNING`, `ERROR`, `CRITICAL`

## Contributing

We welcome contributions to improve compatibility with more filename formats!
If you encounter a filename format that isn't currently supported, consider submitting a pull request or opening an issue.

In particular, we are interested in:

- Adding new filename parsing functions.
- Adding proper tests.
- Improving existing date extraction methods by adding a confidence depending on the accuracy of the extracted timestamp.
- Enhancing error handling and logging.
- A rewrite in rust because rust is always better.

To contribute:
1. Fork the repository.
2. Create a new branch for your feature or bug fix.
3. Implement and test your changes.
4. Submit a pull request with a clear description of your updates.

Your contributions help make Image Date Fixer more robust and useful for everyone!

## License

This project is released under either:

- [MIT License](https://github.com/ink-feather-org/cargo-manifest-proc-macros-rs/blob/main/LICENSE-MIT)
- [Apache License (Version 2.0)](https://github.com/ink-feather-org/cargo-manifest-proc-macros-rs/blob/main/LICENSE-APACHE)

at your choosing.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
