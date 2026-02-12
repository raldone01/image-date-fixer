# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1] - 2026-02-12

- Fix a bug where `exiftool` would not be properly terminated on Unix systems.
- Support respawning `exiftool` workers if they crash.

## [0.2.0] - 2026-02-11

- Support passing files as positional arguments in addition to using the `--files` option.
- Run `exiftool` with the `-stay_open` option to reduce overhead.
- Improve handling of excluded files.

## [0.1.0] - 2025-04-03

Initial release.

[Unreleased]: https://https://github.com/raldone01/image-date-fixer/compare/v0.2.1...HEAD
[0.2.1]: https://https://github.com/raldone01/image-date-fixer/releases/tag/v0.2.0...v0.2.1
[0.2.0]: https://https://github.com/raldone01/image-date-fixer/releases/tag/v0.1.0...v0.2.0
[0.1.0]: https://https://github.com/raldone01/image-date-fixer/releases/tag/v0.1.0
