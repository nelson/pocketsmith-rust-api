# Changelog

## [1.1.0](https://github.com/nelson/pocketsmith-rust-api/compare/pocketsmith-v1.0.0...pocketsmith-v1.1.0) (2026-06-30)


### Features

* **version:** include git commit and build date in version output ([f907f73](https://github.com/nelson/pocketsmith-rust-api/commit/f907f7321ac427bd65eb04a4cf6701a54c0f4f89))

## [1.0.0](https://github.com/nelson/pocketsmith-rust-api/compare/pocketsmith-sync-v0.1.2...pocketsmith-sync-v1.0.0) (2026-06-26)


### ⚠ BREAKING CHANGES

* the per-command binaries (`/usr/local/bin/{sync,serve, transfers,normalise,push,dump,rule}`) no longer exist; invoke them as `pocketsmith <command>` instead. Deploy assets are updated accordingly.

### Features

* add cloud-VM deploy scripts (exe.dev/boxd/sprites) ([1f12039](https://github.com/nelson/pocketsmith-rust-api/commit/1f1203945f6cbc8e8a15187f40f10def2599f720))
* **db:** payee_normalisations staging table ([f587a7c](https://github.com/nelson/pocketsmith-rust-api/commit/f587a7c827d4ce7e9099dbd8dd905fd0bb6effd5))
* **normalise:** apply_confirmed drains confirmed proposals ([65fd39f](https://github.com/nelson/pocketsmith-rust-api/commit/65fd39f3b15a048686986278b8b6dff718c16a7b))
* **normalise:** record per-stage transformation trace; render in serve ([b723dce](https://github.com/nelson/pocketsmith-rust-api/commit/b723dce62f5d2c9254e980fd5889bac77563fa16))
* **normalise:** scan with payee_normalisations staging ([b4881b1](https://github.com/nelson/pocketsmith-rust-api/commit/b4881b13a450aae5c97762d718b05a3725ab374a))
* **push:** per-transaction progress output ([54e6fa4](https://github.com/nelson/pocketsmith-rust-api/commit/54e6fa4545aac5af2e1b2f41942ec1b9d2becb5e))
* **serve/normalise:** action handlers + AppState wiring ([9a3871a](https://github.com/nelson/pocketsmith-rust-api/commit/9a3871a3a93454620885667a6140cd3784a86899))
* **serve/normalise:** filter helpers (status, class, list) ([96a98d4](https://github.com/nelson/pocketsmith-rust-api/commit/96a98d449ca1ead2edba03bf3e7cbc0f7f9a4b96))
* **serve/normalise:** full parity with transfers — decisions, activity, auto-nav ([b535238](https://github.com/nelson/pocketsmith-rust-api/commit/b53523856dbdd93593edb7c0227b0fb71247d3a3))
* **serve/normalise:** polish — layout, parity with transfers, richer detail ([8b1a88c](https://github.com/nelson/pocketsmith-rust-api/commit/8b1a88cdf36f8a73e47456f1dae5917caa86038f))
* **serve/normalise:** views + routes ([abc8e31](https://github.com/nelson/pocketsmith-rust-api/commit/abc8e314d1087655bfa5557f4a9520e41dba0cdb))
* unify CLIs into a single `pocketsmith` binary with subcommands ([c03266c](https://github.com/nelson/pocketsmith-rust-api/commit/c03266cc5fdbad4e3d099de630d588db62380dc6))


### Bug Fixes

* **bin:** all binaries read DB path from POCKETSMITH_DB env ([919e6f5](https://github.com/nelson/pocketsmith-rust-api/commit/919e6f5845d114249920e59bdd724944dcadb532))
* **serve/css:** card-label sizes to content, not a 20x20 square ([4c28421](https://github.com/nelson/pocketsmith-rust-api/commit/4c2842128ec0313fbc77a7d1ebb35227f5c9228c))
* **serve:** tab-key nav, action ordering, card overflow, button spacing ([5af8049](https://github.com/nelson/pocketsmith-rust-api/commit/5af8049deeed822f4efa2768e6264707bdde9fca))
* **sprites:** provision via sprite CLI with idempotent create ([9383ee5](https://github.com/nelson/pocketsmith-rust-api/commit/9383ee525db9abadd11e181b39afa16aecc1c342))
* **sprites:** seed DB only on first run, document re-run behaviour ([d1650dc](https://github.com/nelson/pocketsmith-rust-api/commit/d1650dc71ed8fe3aafac145c2c9da3d8dd921ec1))
* **sprites:** single-line service create, rename to deploy-sprites.sh, auto-deploy on release ([a359560](https://github.com/nelson/pocketsmith-rust-api/commit/a35956068e1f9cf76da0b853b8967e52afbefd7c))
* **sprites:** use sprite-env service manager instead of systemd ([c3464b5](https://github.com/nelson/pocketsmith-rust-api/commit/c3464b5c8539880a951660cd5f373ed8cfc7a5b8))


### Miscellaneous Chores

* release 1.0.0 ([c61dad0](https://github.com/nelson/pocketsmith-rust-api/commit/c61dad0928fb35767fecacec80d25c72c1e63517))

## [0.1.2](https://github.com/nelson/pocketsmith-rust-api/compare/pocketsmith-sync-v0.1.1...pocketsmith-sync-v0.1.2) (2026-06-13)


### Bug Fixes

* **sprites:** seed DB only on first run, document re-run behaviour ([d1650dc](https://github.com/nelson/pocketsmith-rust-api/commit/d1650dc71ed8fe3aafac145c2c9da3d8dd921ec1))
* **sprites:** single-line service create, rename to deploy-sprites.sh, auto-deploy on release ([a359560](https://github.com/nelson/pocketsmith-rust-api/commit/a35956068e1f9cf76da0b853b8967e52afbefd7c))
* **sprites:** use sprite-env service manager instead of systemd ([c3464b5](https://github.com/nelson/pocketsmith-rust-api/commit/c3464b5c8539880a951660cd5f373ed8cfc7a5b8))

## [0.1.1](https://github.com/nelson/pocketsmith-rust-api/compare/pocketsmith-sync-v0.1.0...pocketsmith-sync-v0.1.1) (2026-06-13)


### Bug Fixes

* **sprites:** provision via sprite CLI with idempotent create ([9383ee5](https://github.com/nelson/pocketsmith-rust-api/commit/9383ee525db9abadd11e181b39afa16aecc1c342))

## [0.1.0](https://github.com/nelson/pocketsmith-rust-api/compare/pocketsmith-sync-v0.1.0...pocketsmith-sync-v0.1.0) (2026-06-13)


### Features

* add cloud-VM deploy scripts (exe.dev/boxd/sprites) ([1f12039](https://github.com/nelson/pocketsmith-rust-api/commit/1f1203945f6cbc8e8a15187f40f10def2599f720))
* **db:** payee_normalisations staging table ([f587a7c](https://github.com/nelson/pocketsmith-rust-api/commit/f587a7c827d4ce7e9099dbd8dd905fd0bb6effd5))
* **normalise:** apply_confirmed drains confirmed proposals ([65fd39f](https://github.com/nelson/pocketsmith-rust-api/commit/65fd39f3b15a048686986278b8b6dff718c16a7b))
* **normalise:** record per-stage transformation trace; render in serve ([b723dce](https://github.com/nelson/pocketsmith-rust-api/commit/b723dce62f5d2c9254e980fd5889bac77563fa16))
* **normalise:** scan with payee_normalisations staging ([b4881b1](https://github.com/nelson/pocketsmith-rust-api/commit/b4881b13a450aae5c97762d718b05a3725ab374a))
* **push:** per-transaction progress output ([54e6fa4](https://github.com/nelson/pocketsmith-rust-api/commit/54e6fa4545aac5af2e1b2f41942ec1b9d2becb5e))
* **serve/normalise:** action handlers + AppState wiring ([9a3871a](https://github.com/nelson/pocketsmith-rust-api/commit/9a3871a3a93454620885667a6140cd3784a86899))
* **serve/normalise:** filter helpers (status, class, list) ([96a98d4](https://github.com/nelson/pocketsmith-rust-api/commit/96a98d449ca1ead2edba03bf3e7cbc0f7f9a4b96))
* **serve/normalise:** full parity with transfers — decisions, activity, auto-nav ([b535238](https://github.com/nelson/pocketsmith-rust-api/commit/b53523856dbdd93593edb7c0227b0fb71247d3a3))
* **serve/normalise:** polish — layout, parity with transfers, richer detail ([8b1a88c](https://github.com/nelson/pocketsmith-rust-api/commit/8b1a88cdf36f8a73e47456f1dae5917caa86038f))
* **serve/normalise:** views + routes ([abc8e31](https://github.com/nelson/pocketsmith-rust-api/commit/abc8e314d1087655bfa5557f4a9520e41dba0cdb))


### Bug Fixes

* **bin:** all binaries read DB path from POCKETSMITH_DB env ([919e6f5](https://github.com/nelson/pocketsmith-rust-api/commit/919e6f5845d114249920e59bdd724944dcadb532))
* **serve/css:** card-label sizes to content, not a 20x20 square ([4c28421](https://github.com/nelson/pocketsmith-rust-api/commit/4c2842128ec0313fbc77a7d1ebb35227f5c9228c))
* **serve:** tab-key nav, action ordering, card overflow, button spacing ([5af8049](https://github.com/nelson/pocketsmith-rust-api/commit/5af8049deeed822f4efa2768e6264707bdde9fca))


### Miscellaneous Chores

* release 0.1.0 ([c61dad0](https://github.com/nelson/pocketsmith-rust-api/commit/c61dad0928fb35767fecacec80d25c72c1e63517))
