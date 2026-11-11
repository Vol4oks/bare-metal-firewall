---
tags: [toolchain, just, ci, qemu, тесты]
aliases: [justfile, just recipes]
stage: 5
---

# `justfile`: сборка, линт и smoke-прогоны

> [!abstract] Суть
> `just` — единая точка входа для типовых команд проекта. `check` не запускает QEMU (fmt, clippy во всех конфигурациях, debug/release). `smoke` запускает четыре QEMU-сценария и проверяет **маркеры в логе**, потому что QEMU сам не завершается. В smoke QEMU запускается **напрямую** с `-serial file:`, а не через `cargo run`: иначе stdio-монитор ведёт себя непредсказуемо в неинтерактивном режиме, а `timeout` убивает cargo, а не QEMU.

## Почему не `cargo run` в smoke

В `.cargo/config.toml` runner — `qemu-system-aarch64 … -nographic -serial mon:stdio`. Для интерактивной работы это правильно. Для автоматики — три проблемы:

1. `mon:stdio` мультиплексирует монитор и UART; при неинтерактивном stdin/stdout QEMU может закрыться до того, как гость что-то напечатает.
2. stdout QEMU может буферизоваться, если это файл, а не tty: при убийстве процесса буфер теряется.
3. `timeout` посылает сигнал **cargo**, а не QEMU; QEMU может пережить cargo.

Поэтому smoke-рецепты зовут QEMU напрямую:

```text
qemu-system-aarch64 -M virt,gic-version=3 -cpu cortex-a55 -smp 4 -m 1024M \
  -display none -monitor none -serial file:target/logs/<case>.log \
  -kernel target/aarch64-unknown-none/debug/bare-metal-firewall
```

`timeout` ограничивает жизнь самого QEMU, UART пишется прямо в файл, stdio не участвует.

> [!warning] Дублирование runner
> Параметры QEMU теперь есть в двух местах: `.cargo/config.toml` (интерактив) и `justfile` (smoke). Менять их нужно синхронно. После перехода на workspace проще будет генерировать оба из `xtask`, но пока дублирование дешевле.

## Рецепты

| Рецепт | Что делает |
|---|---|
| `just` | список рецептов |
| `just build` / `just release` | сборка debug / release |
| `just fmt` / `just fmt-check` | форматирование / проверка |
| `just clippy` | clippy × 4 конфигурации фич |
| `just check` | fmt-check + clippy + build + release |
| `just run*` | интерактивные прогоны (Ctrl-C) |
| `just smoke` | четыре QEMU-сценария с проверкой маркеров |
| `just ci` | `check` + `smoke` |
| `just symbols` | `nm` по asm-символам |

Логи — в `target/logs/*.log` (`target/` в `.gitignore`).

## Файл

> [!warning] Листинг ниже — состояние до перехода на workspace
> Актуальный файл — в репозитории. После 1.1–1.2 отличаются только эти строки:
>
> ```just
> kernel := "target/aarch64-unknown-none/debug/firmware"
>
> build:
>     cargo fw
> release:
>     cargo fw --release
> run:
>     cargo fwr
> run-selftest:
>     cargo fwr --features selftest
> run-schedtest:
>     cargo fwr --features schedtest
>
> # в smoke-рецептах: cargo build … → cargo fw …
> # в symbols: путь к бинарю → …/debug/firmware
> ```
>
> Блок `clippy` теперь не может быть «голым»: без `[build] target` он соберёт `firmware` под host и упадёт:
>
> ```just
> clippy:
>     cargo clippy --target aarch64-unknown-none -p firmware -- -D warnings
>     cargo clippy --target aarch64-unknown-none -p firmware --features selftest -- -D warnings
>     cargo clippy --target aarch64-unknown-none -p firmware --features schedtest -- -D warnings
>     cargo clippy --target aarch64-unknown-none -p firmware --features selftest,schedtest -- -D warnings
> ```
>
> Через `fwc` нельзя: дописанные аргументы уходят после `--`, то есть rustc, а не cargo.

```just
# justfile — сборка, линт и smoke-прогоны в QEMU
# Установка: sudo apt install just   или   cargo install just
# Список рецептов: just
# Таймаут: FW_TIMEOUT=20 just smoke

set shell := ["sh", "-c"]

tmout  := env("FW_TIMEOUT", "12")
logdir := "target/logs"

# Прямой QEMU: без stdio-монитора, чтобы smoke не зависел от stdin/tty
qemu   := "qemu-system-aarch64 -M virt,gic-version=3 -cpu cortex-a55 -smp 4 -m 1024M -display none -monitor none"
kernel := "target/aarch64-unknown-none/debug/bare-metal-firewall"

default:
    @just --list

# ---------- сборка ----------

build:
    cargo build

release:
    cargo build --release

clean:
    cargo clean

# ---------- формат и clippy ----------

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

# clippy во всех конфигурациях фич
clippy:
    cargo clippy -- -D warnings
    cargo clippy --features selftest -- -D warnings
    cargo clippy --features schedtest -- -D warnings
    cargo clippy --features selftest,schedtest -- -D warnings

# быстрая проверка без QEMU
check: fmt-check clippy build release
    @echo "check: OK"

# ---------- интерактивные прогоны (выход — Ctrl-C) ----------

run:
    cargo run

run-selftest:
    cargo run --features selftest

run-schedtest:
    cargo run --features schedtest

# ---------- smoke: QEMU запускается напрямую ----------
# timeout ожидаем (код 124); решение принимается по маркерам в логе.

smoke-default:
    #!/usr/bin/env sh
    set -eu
    mkdir -p {{logdir}}
    log={{logdir}}/default.log
    cargo build >/dev/null
    rm -f "$log"
    timeout {{tmout}} {{qemu}} -serial file:"$log" -kernel {{kernel}} >/dev/null 2>&1 || true
    [ -f "$log" ] || { echo "FAIL default: qemu did not start"; exit 1; }
    if grep -q PANIC "$log"; then echo "FAIL default: panic"; tail -20 "$log"; exit 1; fi
    grep -q "Hello from QEMU virt" "$log" || { echo "FAIL default: no banner"; tail -20 "$log"; exit 1; }
    grep -q "ticks = " "$log" || { echo "FAIL default: no ticks"; tail -5 "$log"; exit 1; }
    echo "PASS default ($log)"

smoke-selftest:
    #!/usr/bin/env sh
    set -eu
    mkdir -p {{logdir}}
    log={{logdir}}/selftest.log
    cargo build --features selftest >/dev/null
    rm -f "$log"
    timeout {{tmout}} {{qemu}} -serial file:"$log" -kernel {{kernel}} >/dev/null 2>&1 || true
    [ -f "$log" ] || { echo "FAIL selftest: qemu did not start"; exit 1; }
    if grep -q PANIC "$log"; then echo "FAIL selftest: panic"; tail -20 "$log"; exit 1; fi
    grep -q "sched: model ok" "$log" || { echo "FAIL selftest: policy"; tail -20 "$log"; exit 1; }
    grep -q "selftest: end" "$log" || { echo "FAIL selftest: no end"; exit 1; }
    grep -q "ticks = " "$log" || { echo "FAIL selftest: no ticks"; exit 1; }
    echo "PASS selftest ($log)"

smoke-schedtest:
    #!/usr/bin/env sh
    set -eu
    mkdir -p {{logdir}}
    log={{logdir}}/schedtest.log
    cargo build --features schedtest >/dev/null
    rm -f "$log"
    timeout {{tmout}} {{qemu}} -serial file:"$log" -kernel {{kernel}} >/dev/null 2>&1 || true
    [ -f "$log" ] || { echo "FAIL schedtest: qemu did not start"; exit 1; }
    if grep -q PANIC "$log"; then echo "FAIL schedtest: panic"; tail -20 "$log"; exit 1; fi
    grep -q "schedtest: start" "$log" || { echo "FAIL schedtest: no start"; tail -20 "$log"; exit 1; }
    grep -q "sched: canary ok" "$log" || { echo "FAIL schedtest: canary"; exit 1; }
    grep -q "ticks = " "$log" || { echo "FAIL schedtest: no ticks"; exit 1; }
    echo "PASS schedtest ($log)"

smoke-both:
    #!/usr/bin/env sh
    set -eu
    mkdir -p {{logdir}}
    log={{logdir}}/both.log
    cargo build --features selftest,schedtest >/dev/null
    rm -f "$log"
    timeout {{tmout}} {{qemu}} -serial file:"$log" -kernel {{kernel}} >/dev/null 2>&1 || true
    [ -f "$log" ] || { echo "FAIL both: qemu did not start"; exit 1; }
    if grep -q PANIC "$log"; then echo "FAIL both: panic"; tail -20 "$log"; exit 1; fi
    grep -q "sched: model ok" "$log" || { echo "FAIL both: policy"; tail -20 "$log"; exit 1; }
    grep -q "sched: canary ok" "$log" || { echo "FAIL both: canary"; exit 1; }
    grep -q "ticks = " "$log" || { echo "FAIL both: no ticks"; exit 1; }
    echo "PASS both ($log)"

# все QEMU-сценарии
smoke: smoke-default smoke-selftest smoke-schedtest smoke-both

# полная проверка перед коммитом
ci: check smoke
    @echo "CI: OK"

# глобальные символы asm в собранном образе
symbols: build
    nm target/aarch64-unknown-none/debug/bare-metal-firewall | grep -E "sched_switch|sched_start"

# Негативные сценарии (маска / счётчик / без waker) требуют временной правки кода:
# см. doc/review/stage5_0_4_review.md
```

## Грабли

> [!warning]
> - `timeout` возвращает 124 — это **норма**, поэтому `|| true`; решение по маркерам.
> - Без `rm -f "$log"` старый лог от прошлого прогона даст ложный PASS.
> - `cargo build` стоит **до** `timeout`: ошибка компиляции не должна маскироваться таймаутом.
> - Проверка `PANIC` идёт раньше остальных: иначе «нет тиков» может означать падение.
> - Если QEMU не стартовал, `-serial file:` не создаст лог — это ловится `[ -f "$log" ]`.
> - Негативные сценарии не автоматизируются: они требуют правки кода. Когда появится workspace, их можно вынести в фичу `hog`.
> - **«no banner» на пустом логе = неверный путь `kernel`, а не проблема гостя.** `-serial file:` создаёт файл ещё до загрузки ядра, поэтому проверка `[ -f "$log" ]` проходит, а QEMU молча падает на отсутствующем файле (stderr уходит в `/dev/null`). Сначала сверить имя бинаря: после workspace это `…/debug/firmware`, а не `…/debug/bare-metal-firewall`.

## Связанные заметки
- [[cargo_aliases]], [[qemu_adaptation]], [[no_std_testing]], [[stage5_0_4_review]], [[roadmap_5_12]]
