---
tags: [toolchain, tftp, плата, скрипты, justfile, probe]
aliases: [board scripts, загрузка на плату, tftp serve]
stage: 1
---

# Скрипты: сборка `probe/` и загрузка на плату

> [!abstract] Суть
> Цикл на плате = «собрать плоский `.bin` → отдать по TFTP → `go` по базе линковки». Всё остальное — обвязка: правильный таргет и `-T`, `objcopy`, статический IP на хосте, TFTP-сервер, и понимание, что env U-Boot не сохраняется.

## Предпосылки

- `probe/` собран по [[h2_minimal_image]] и лежит рядом с корнем проекта.
- Кабель — в **WAN-порт (ближайший к USB-C)**: в U-Boot есть только GMAC `eth0`, RTL8111H там не появится никогда.
- На хосте TFTP-сервер и адрес `192.168.1.1`. Плата получает `192.168.1.10`.

## Сборка образа

```bash
cd probe
RUSTFLAGS="-C link-arg=-Tlinker-rk3566.ld" \
    cargo build --release --target aarch64-unknown-none
rust-objcopy -O binary \
    target/aarch64-unknown-none/release/probe-h2 fw.bin
```

> [!warning] Почему именно так
> - `RUSTFLAGS` **отменяет** `rustflags` из родительского `.cargo/config.toml`, иначе подставится `-Tlinker.ld` с базой `0x42000000`.
> - `target/aarch64-unknown-none/release/probe-h2` — ELF; `go` его не разбирает, нужен `objcopy -O binary`.
> - Если в системе нет `rust-objcopy`: `llvm-objcopy` или `aarch64-linux-gnu-objcopy`.

## Предполётная проверка

```bash
# 1. файл не пустой и не ELF
xxd -l 16 probe/fw.bin

# 2. .text по базе линковки
llvm-objdump -h probe/target/aarch64-unknown-none/release/probe-h2 \
    | grep -E "\.text|\.bss"

# 3. размер разумный (десятки КБ, не мегабайты)
ls -l probe/fw.bin
```

Признак ошибки №1: первые байты `7f 45 4c 46` (`\x7fELF`) — отправили ELF.

## Хост: адрес и TFTP

```bash
# адрес на интерфейсе, куда подключена плата (имя посмотреть в `ip link`)
sudo ip addr add 192.168.1.1/24 dev enp3s0
sudo ip link set enp3s0 up
```

### Вариант A: dnsmasq (одна команда, без демона)

```bash
sudo dnsmasq --no-daemon --port=0 \
    --enable-tftp --tftp-root="$PWD/probe" \
    --interface=enp3s0 --bind-interfaces
```

### Вариант B: tftpd-hpa

```bash
sudo cp probe/fw.bin /srv/tftp/
sudo systemctl restart tftpd-hpa
# проверить: tftp 192.168.1.1 -c get fw.bin
```

> [!warning] Firewall
> Если включён `ufw`/`nftables`, разрешить UDP/69 и диапазон эфемерных портов, иначе будет `TFTP error: timeout`.

## U-Boot: последовательность

Env энергонезависимого нет (`Loading Environment from nowhere`), поэтому IP задаётся при каждом включении:

```text
setenv ipaddr 192.168.1.10
setenv serverip 192.168.1.1
tftp 0x02000000 fw.bin
go 0x02000000
```

Проверка, что линк есть до TFTP:

```text
ping 192.168.1.1
```

Ожидаемо: `E2H1G` в консоли. Возврат — power-cycle (свой `wfi` управление не отдаёт).

## Рецепты `justfile` (предлагаемые)

```make
# адрес платы и адрес хоста
board_ip    := "192.168.1.10"
host_ip     := "192.168.1.1"
tftp_dir    := "probe"
probe_elf   := "probe/target/aarch64-unknown-none/release/probe-h2"
probe_bin   := "probe/fw.bin"

probe-build:
    cd probe && RUSTFLAGS="-C link-arg=-Tlinker-rk3566.ld" \
        cargo build --release --target aarch64-unknown-none

probe-bin: probe-build
    rust-objcopy -O binary {{probe_elf}} {{probe_bin}}
    @ls -l {{probe_bin}}

probe-check: probe-bin
    @xxd -l 16 {{probe_bin}} | grep -qv "7f 45 4c 46" || { echo "ELF, нужен objcopy"; exit 1; }
    @echo "probe: OK"

tftp-serve:
    sudo dnsmasq --no-daemon --port=0 --enable-tftp --tftp-root="{{tftp_dir}}" \
        --interface={{host_if}} --bind-interfaces

board-hint:
    @echo "U-Boot:"
    @echo "  setenv ipaddr {{board_ip}}"
    @echo "  setenv serverip {{host_ip}}"
    @echo "  tftp 0x02000000 fw.bin"
    @echo "  go 0x02000000"
```

`host_if` надо задать переменной вверху файла — имя интерфейса у каждого своё.

## Диагностика

| Симптом | Причина | Что делать |
|---|---|---|
| `TFTP error: 'File not found'` | не тот корень/имя | сверить `--tftp-root` и имя `fw.bin` |
| `TFTP error: timeout` | firewall, не тот порт, нет линка | `ping 192.168.1.1` из U-Boot; разрешить UDP/69 |
| `ping` не идёт | кабель в RTL8111H | переставить в WAN (у USB-C) |
| загрузилось, но тишина | `go` не по базе линковки / ELF | сверить `0x02000000` с `probe/linker-rk3566.ld`; `xxd` |
| `E2` без `H1` | сломан `eret` | `SPSR_EL2=0x3c5`, `ELR_EL2`, `HCR_EL2.RW` |
| мусор в консоли | не та скорость | 1500000; линия уже проверена loopback |

## Гигиена репозитория

В `.gitignore` не хватает: `probe/target`, `*.img`, `*.img.gz`, `picocom.log`, `probe/fw.bin`. Сейчас в `git status` висят образ OpenWrt и лог консоли — их коммитить не нужно.

## Связанные заметки
- [[h2_minimal_image]], [[uboot_loading]], [[serial_console]], [[sd_card_image]], [[board_bringup]], [[memory_map_rk3566]]
