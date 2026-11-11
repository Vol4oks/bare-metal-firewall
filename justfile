# justfile — сборка, линт и smoke-прогоны в QEMU
# Установка: sudo apt install just   или   cargo install just
# Список рецептов: just
# Таймаут: FW_TIMEOUT=20 just smoke

set shell := ["sh", "-c"]

tmout  := env("FW_TIMEOUT", "12")
logdir := "target/logs"
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

# ---------- smoke: QEMU сам не завершается ----------
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
