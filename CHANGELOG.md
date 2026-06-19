# Changelog

## [0.6.3](https://github.com/butterflyskies/cc-toolgate/compare/v0.6.2...v0.6.3) (2026-06-19)


### Bug Fixes

* cross-list dedup in config merge to prevent allow/ask/deny conflicts ([#68](https://github.com/butterflyskies/cc-toolgate/issues/68)) ([26b0f39](https://github.com/butterflyskies/cc-toolgate/commit/26b0f398f36e5f6385ff0985b5cb6d5dff7244dc))

## [0.6.3](https://github.com/butterflyskies/cc-toolgate/compare/v0.6.2...v0.6.3) (2026-06-19)


### Bug Fixes

* **config:** cross-list dedup in config merge — when a command is promoted to a higher-priority list (e.g. user adds `curl` to `allow`), it is now automatically removed from lower-priority lists (`ask`, `deny`). Previously, the command remained in both lists after merge, and the registry's last-writer-wins insertion order caused the lower-priority entry to shadow the user's override.

## [0.6.2](https://github.com/butterflyskies/cc-toolgate/compare/v0.6.1...v0.6.2) (2026-05-29)


### Features

* **config:** add project-level config overlay ([#50](https://github.com/butterflyskies/cc-toolgate/issues/50)) ([9724ad1](https://github.com/butterflyskies/cc-toolgate/commit/9724ad1cac97631363a82594bf116158e54aa17b)) — thanks to [@diminishedprime](https://github.com/diminishedprime) for the contribution
* **eval:** annotate ASK decisions with project overlay provenance
* **security:** strip `replace`/`remove_*` from project overlays — project configs can only add permissions, never weaken user-global rules

## Changelog
