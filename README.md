# `image-date-fixer`

[![crates.io](https://img.shields.io/crates/v/image-date-fixer.svg)](https://crates.io/crates/image-date-fixer)

`image-date-fixer` is a command-line tool written in Rust for restoring missing exif data from the filepath.
This can be very useful for tagging photos received from friends or family, or for photos from legacy devices that do not support or strip exif data.

> ⚠️ **Warning:** Always back up your files before running any script that modifies them. Test the script with the `--dry-run` option first to ensure it behaves as expected.

## Features

- Extracts date information from filenames of various formats, including:
  - Android-style (`IMG_YYYYMMDD_HHMMSS.jpg`)
  - Standard date-prefixed (`YYYY-MM-DD_HHMMSS.jpg`)
  - Screenshot-style (`Screenshot_YYYYMMDD-HHMMSS.jpg`)
  - Unix timestamp-prefixed filenames
  - UUID timestamp-prefixed filenames
  - WhatsApp-style (`IMG-YYYYMMDD-WAXXXX.jpg`)
- Respects existing EXIF metadata if available
- Corrects invalid file modification dates
- Supports recursive processing of directories
- Can exclude specific directories from processing
- Dry-run mode for testing without modifying files
- `--print-stats` will show a summary of the changes made
- Use `--help` to see all available options

## Example usage - from binary

Make sure to add `~/.cargo/bin` to your `PATH`.

```shell
cargo install image-date-fixer
image-date-fixer --print-stats --ignore-minor-exif-errors  --log-level INFO --fix-future-modified-times 2 --fix-future-exif-dates 2 --files /my_folder_with_images --exclude-files /my_folder_with_images/ignored --dry-run
```


## Example usage - from source

```shell
git clone <this repo>
cd image-date-fixer
cargo build --release
./target/release/image-date-fixer --print-stats --ignore-minor-exif-errors  --log-level INFO --fix-future-modified-times 2 --fix-future-exif-dates 2 --files /my_folder_with_images --exclude-files /my_folder_with_images/ignored --dry-run
```

## Contributing

We welcome contributions to improve compatibility with more filename formats!
If you encounter a filename format that isn't currently supported, consider submitting a pull request or opening an issue.

In particular, we are interested in:

- Adding new filename parsing functions.
- Enhancing error handling and logging.
- Improving false positive rates.
- Adding tests with real files.
- Fixing more clippy warnings.
- Switching to bindings for `libexif` for a massive speedup.
- Support more parent directory levels.
- Support asking ollama for a smarter date resolution powered by ai.
- Add support for limiting the amount of parallel exiftool calls to increase responsiveness of the system.

Your contributions help make `image-date-fixer` more robust and useful for everyone!

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
