---
tags: [toolchain, sd, образ, uboot, этап-1]
aliases: [SD image, запись карты, Etcher, dd]
stage: 1
---

# SD-карта: вендорский образ

> [!abstract] Суть
> На плате без eMMC SD-карта — единственный носитель и одновременно точка восстановления. Вендорский образ FriendlyElec уже содержит idbloader + U-Boot + Linux, поэтому запись карты закрывает сразу H0a/H0b (баннер и команды) и даёт Linux-факты (DTB, `/proc/iomem`, `dmesg`). Главные ошибки — образ не для SD и неверное устройство в `dd`.

## Что скачать

- Страница загрузок FriendlyElec для **NanoPi R3S LTS** (не R2S/R5S).
- Образ **для SD-карты** (`.img.gz` / `.img.xz`), не eFlasher/eMMC-вариант.
- Сверить контрольную сумму, если опубликована.

> [!warning] Вариант образа
> Редакция (Debian/Ubuntu, minimal/desktop) роли не играет: нужны U-Boot, ядро и DTB. А вот eMMC-вариант может не содержать корректного IDB для SD — тогда U-Boot просто не появится.

## Если сайт вендора недоступен: OpenWrt

OpenWrt годится: для rockchip/armv8 он ставит mainline U-Boot, а консоль у этих плат тоже 1500000. Но требования к файлу жёстче.

- Нужен **полный образ диска** для SD: `*-sysupgrade.img.gz` / `*-factory.img.gz` (внутри должен быть idbloader + U-Boot).
- У OpenWrt имя `*-squashfs-sysupgrade.img` — это **полный образ диска** (SPL + U-Boot + rootfs), и `dd` на карту — штатный способ. Осторожность про «sysupgrade-архив» относится только к `.tar`.
- Перед записью проверить, что загрузчик внутри есть:

```bash
# распаковать (если .gz/.xz)
gunzip -k image.img.gz

# 1) разметка: первый раздел обычно с 32 МиБ и меньше
sudo fdisk -l image.img

# 2) главный признак: строка U-Boot в первых 32 МиБ
sudo dd if=image.img bs=1M count=32 2>/dev/null | strings | grep -i -m3 "u-boot"
```

- Отличия от вендорского образа: U-Boot mainline (набор команд может быть уже), нет вендорского env, `bootdelay` обычно 1 секунда — прерывать автозагрузку надо сразу после подачи питания.
- Сверить модель платы в дереве: `/proc/device-tree/model` и `compatible`. Образ может быть собран под R3S, а не под R3S LTS; тогда DTB и часть периферии могут не совпасть.

## Запись

```bash
# 1. Найти карту: смотреть SIZE и MODEL, а не «первое usb-устройство»
lsblk -o NAME,SIZE,MODEL,TRAN

# 2. Размонтировать, если смонтировалась автоматически
sudo umount /dev/sdX?*

# 3a. dd
sudo dd if=nanopi-r3s-lts-*.img of=/dev/sdX bs=4M conv=fsync status=progress
sync

# 3b. либо balenaEtcher — то же самое с защитой от выбора системного диска
```

- `/dev/sdX` — **вся карта**, не раздел (`/dev/sdX1` — ошибка).
- Разметку и ФС руками не создавать: они внутри образа.

> [!warning] Цена ошибки
> `dd` на системный диск необратим. Карта обычно `TRAN=usb`, системный диск — `sata`/`nvme`. Сверить `lsblk` дважды.

## Первый запуск

1. Карта вставлена, UART подключён 3.3 В / GND / TX↔RX ([[serial_console]]).
2. **USB к ПК отключить** — он был нужен только для Maskrom; питание от блока питания.
3. Открыть picocom на 1500000 с логом, затем подать питание.
4. Снять весь лог автозагрузки, не нажимая клавиш: `U-Boot 20xx.xx`, `DRAM:`, `Loading Environment from …`, `MMC:`, `SF:`, `Net:`.
5. Остановить автозагрузку и выполнить H0b ([[h0_fact_sheet]]):

```text
bdinfo
printenv
mmc info
sf probe
pci enum
pci
```

## Факты из Linux

```bash
cat /proc/iomem | grep -i "System RAM"
dmesg | grep -iE "started at EL|arch_timer|gmac|8169|mmc0"
cat /proc/cpuinfo | head -20
ip link                  # busybox-ip в OpenWrt не знает -br
ls /sys/class/net

# какой интерфейс на каком драйвере: stmmac/rk_gmac против r8169
for n in /sys/class/net/*; do
  echo "$(basename $n): $(readlink -f $n/device/driver 2>/dev/null | xargs -r basename)"
done

cat /proc/device-tree/model; echo

# DTB: в OpenWrt /proc/device-tree может отсутствовать — тогда брать через /sys/firmware/fdt
cat /sys/firmware/fdt > /tmp/board.dtb

# dtc может быть не установлен: opkg install dtc, иначе разобрать образ/DTB на ПК
dtc -I dtb -O dts /tmp/board.dtb > /tmp/board.dts
grep -nE "serial@|interrupt-controller@|memory@|ethernet@|clock-frequency" /tmp/board.dts
```

Логин: у вендорского образа — по странице образа (обычно `pi`/`pi`), у OpenWrt на консоли root входит без пароля. Если не входит — не перебирать, смотреть страницу образа.

Сеть в OpenWrt для будущего TFTP:

```bash
ip addr add 192.168.1.10/24 dev eth0
ip link set eth0 up
tftp -g -r fw.bin 192.168.1.1     # busybox tftp
```

## Куда складывать

`doc/hardware/logs/`: `h0a-banner.log`, `h0b-uboot.log`, `linux-facts.txt`. Как есть: по ним заполняется [[h0_fact_sheet]].

## Подводные камни

> [!warning]
> - `mmc info` покажет базу SD-контроллера (`mmc@…`) — пригодится на этапе 12.
> - Не прошивать SPI NOR на этом шаге: SD — единственная страховка.
> - Если U-Boot не появился: проверить `mmc info` (видит ли карту), тип образа, и только потом грешить на UART — линия уже проверена loopback'ом.
> - Карта, записанная «наполовину» (`dd` без `sync`/`conv=fsync`), даёт именно такую картину: тишина или обрыв на середине баннера.

## Связанные заметки
- [[h0_fact_sheet]], [[board_bringup]], [[rk3566_bootrom_media]], [[serial_console]], [[uboot_loading]]
