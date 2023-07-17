# Changelog

<!-- https://keepachangelog.com/en/1.0.0/ -->

## [0.3.2] - 2023-07-17

- Bump dependencies

## [0.3.1] - 2023-04-01

- Releases now include OSX executables

## [0.3.0] - 2023-03-18

### Added

- Added a flag to produce a Markdown issue comment
- Dockerfile
- Icemelter now runs `rustfmt` on the reduced file and keeps the result if it
  maintains the ICE.
- Integration with cargo-bisect-rustc
- Icemaker can now fetch MCVEs directly from Github issues when compiled with
  `--features=fetch`

### Changed

- Improved error messages in a variety of situations

## [0.2.0] - 2023-03-17

### Added

- Logging

### Changed

- Icemelter will now avoid introducing spurrious errors by default

## [0.1.0] - 2023-03-16

Initial release!

[0.1.0]: https://github.com/langston-barrett/icemelter/releases/tag/v0.1.0
[0.2.0]: https://github.com/langston-barrett/icemelter/releases/tag/v0.2.0
[0.3.0]: https://github.com/langston-barrett/icemelter/releases/tag/v0.3.0
[0.3.1]: https://github.com/langston-barrett/icemelter/releases/tag/v0.3.1
[0.3.2]: https://github.com/langston-barrett/icemelter/releases/tag/v0.3.2
