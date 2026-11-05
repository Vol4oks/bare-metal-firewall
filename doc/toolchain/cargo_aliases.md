---
tags: [toolchain, cargo, workspace, тесты, qemu]
aliases: [cargo aliases, fw alias, host tests]
stage: 5
---

# Сборка через алиасы: `cargo fw` / `fwt`

> [!abstract] Суть
> В bare-metal workspace глобальный `[build] target` удобен для `cargo run`, но делает невозможными нативные host-тесты: `cargo test -p fw-sched` тоже уходит в кросс-компиляцию и пытается запустить тестовый бинарник через QEMU-runner. Решение (вариант 2B): убрать `[build] target`, а платформу указывать в алиасах. Прошивка — `cargo fw`/`fwr`, тесты — `cargo fwt`, clippy прошивки — `cargo fwc`. Плата: «голые» `cargo build`/`cargo run` больше не собирают прошивку.

## Конфигурация

`.cargo/config.toml`:

```toml
[target.aarch64-unknown-none]
linker = "rust-lld"
runner = "qemu-system-aarch64 -M virt,gic-version=3 -cpu cortex-a55 -smp 4 -m 1024M -nographic -serial mon:stdio -kernel"

[alias]
fw  = "build  --target aarch64-unknown-none -p firmware"
fwr = "run    --target aarch64-unknown-none -p firmware"
fwc = "clippy --target aarch64-unknown-none -p firmware -- -D warnings"
fwt = "test -p fw-sync"
```

- `[build] target` отсутствует — это принципиально.
- `rustflags` с `-Tlinker.ld` тоже отсутствуют: линкерный аргумент выдаёт `firmware/build.rs` (см. ниже). Относительный путь в `rustflags` разрешается относительно каталога запуска, а не манифеста, и ломается при сборке из подкаталога.
- Алиасы — это просто подкоманда с аргументами; `-p firmware` обязателен, потому что в workspace несколько пакетов.

## Что меняется в привычках

| Было | Стало | Почему |
|---|---|---|
| `cargo run` | `cargo fwr` | без `[build] target` нужен явный `--target` |
| `cargo build` | `cargo fw` | иначе соберётся host-вариант и упадёт |
| `cargo clippy` | `cargo fwc` (прошивка) / `cargo clippy -p fw-sched --all-targets` (lib) | clippy должен знать таргет |
| `cargo test` | `cargo fwt` | host-тесты требуют host-таргета |
| `cargo fmt` | `cargo fmt` | не зависит от таргета |

`fwt` расширяется по мере появления host-тестируемых крейтов:

```toml
fwt = "test -p fw-sync -p fw-sched -p fw-config -p fw-firewall -p fw-net"
```

Пакеты, которых ещё нет, в алиас добавлять нельзя — команда падает на разрешении графа.

## Почему тесты начинают работать

- Без `--target` cargo собирает под host (`x86_64-unknown-linux-gnu`), и `#[test]`-харнесс доступен.
- Секция `[target.aarch64-unknown-none]` (linker, runner, rustflags) **не применяется** к host-сборке: префикс по триплу.
- `runner` вызывается только для того таргета, под которым объявлен; host-тесты запускаются нативно.

## `default-members`

В корневом `Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["firmware", "crates/*"]
default-members = ["crates/fw-sync"]
```

Даёт две вещи:
- «голый» `cargo build`/`cargo test` из корня работает с host-крейтами и не пытается собрать `firmware` под хост;
- прошивка собирается только осознанно, через `cargo fw`.

Список `default-members` и `fwt` обновляются вместе.

## Линкерный скрипт: `build.rs`

`firmware/build.rs`:

```rust
fn main() {
    println!("cargo::rerun-if-changed=../linker.ld");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("none") {
        let root = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        println!("cargo::rustc-link-arg=-T{root}/../linker.ld");
    }
}
```

- `CARGO_CFG_TARGET_OS == "none"` — единственный признак bare-metal, который видит build-скрипт. Без проверки host-сборка получит `-T` и упадёт.
- Абсолютный путь снимает вопрос «откуда запущен cargo».
- `rerun-if-changed` — правка `linker.ld` пересобирает образ, а не игнорируется.
- `cargo::` (двойное двоеточие) требует Cargo ≥ 1.77; edition 2024 и так требует 1.85+.

## rust-analyzer

`.zed/settings.json` уже настроен правильно для этого варианта:

```json
{
  "lsp": {
    "rust-analyzer": {
      "initialization_options": {
        "cargo": {
          "target": "aarch64-unknown-none",
          "allTargets": false,
          "unsetTest": true
        },
        "check": { "target": "aarch64-unknown-none", "allTargets": false }
      }
    }
  }
}
```

- `cargo.target` — RA анализирует код под bare-metal, а не под host (иначе asm и `#![no_main]` дают ложные ошибки).
- `unsetTest: true` — RA не пытается анализировать `#[cfg(test)]`-модули host-крейтов под bare-metal (там `std::thread` и `Arc`, которых нет).
- `allTargets: false` — не тянет примеры/тесты/бенчи в анализ.

Итог: **cargo** гоняет host-тесты нативно, **RA** при этом остаётся на aarch64 — это и есть смысл разделения «сборка» и «анализ».

## Диагностика

| Симптом | Причина | Лечение |
|---|---|---|
| `cannot open linker script file linker.ld` | относительный `-T` или запуск из подкаталога | `-T` только из `build.rs` с абсолютным путём |
| `cargo test -p fw-sched` пытается запустить QEMU | остался `[build] target` | убрать его, тесты — через `fwt` |
| `profiles for the non root package will be ignored` | `[profile.*]` в манифесте члена | профили только в корне; иначе `panic = "abort"` не применится |
| `duplicate lang item panic_impl` | `#[panic_handler]` в библиотеке | handler только в `firmware` |
| `e0463 can't find crate for core` | сборка lib под host без `std`-режима | `#![cfg_attr(not(test), no_std)]` |
| host-тест видит `aarch64`-asm | asm не загейчен | `#[cfg(target_arch = "aarch64")]` |
| `error: unrecognized option -T...` при `cargo build` | `build.rs` не проверил `CARGO_CFG_TARGET_OS` | добавить проверку |

> [!warning]
> Алиас не виден в автодополнении IDE и не работает из подкаталога с другим `.cargo/config.toml`. Если решите перейти на `xtask`, единственное, что изменится, — способ вызова: таргет и `-p` останутся теми же.

## Связанные заметки
- [[0008_workspace_layout]], [[roadmap_5_12]], [[no_std_testing]], [[qemu_adaptation]]
