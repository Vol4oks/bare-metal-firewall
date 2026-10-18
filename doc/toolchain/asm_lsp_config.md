---
tags: [toolchain, zed, asm-lsp, диагностика, редактор]
aliases: [asm-lsp, ложные ошибки ассемблера]
stage: 3
---

# asm-lsp: диагностика редактора против компилятора

> [!abstract] Суть
> Расширение Zed «Assembly» поднимает asm-lsp, который по умолчанию думает x86/x86-64 и зовёт системный gcc для диагностики. Для aarch64-файлов он даёт ложные ошибки вида «no such instruction: b unexpected». Источник истины — `cargo build`: rustc ассемблирует через LLVM с целевой архитектурой.

## Механика ложной ошибки

1. Дефолтный конфиг asm-lsp: `instruction_set = "x86/x86-64"`, диагностика через системный gcc/clang.
2. Host-gcc на x86_64 не знает `b` — это мнемоника AArch64 (в x86 переход — `jmp`).
3. cargo при этом собирает тем же файлом без проблем: `global_asm!`/rustc использует LLVM IAS с `-target aarch64-unknown-none` из `.cargo/config.toml`.

Две одинаковые ошибки на одну причину: диагностика агрегируется по вхождениям развёрнутого макроса `SLOT`.

## Фикс (`.asm-lsp.toml` в корне проекта)

```toml
[default_config]
assembler = "gas"
instruction_set = "arm64"

[opts]
diagnostics = false
default_diagnostics = false
```

- Подсветка (tree-sitter) и hover/автодополнение по ARM64 сохраняются.
- Диагностика выключена: без кросс-тулчейна она всё равно была бы x86-механикой.
- После правки — рестарт языкового сервера (`editor: restart language server`).

> [!warning] Подводные камни
> - ЛСП-диагностика калибруется под тулчейн конфигом; арбитр — компилятор. «Редактор ругается, а cargo зелёный» = сначала проверь, той ли архитектурой думает LSP.
> - «Настоящая» диагностика в редакторе требует кросс-тулчейна (`gcc-aarch64-linux-gnu` + `compile_flags.txt`) — избыточно, пока есть cargo build + readelf.

## Связанные заметки
- [[a64_assembly_basics]], [[vector_table_31]], [[stage3_interrupts]], [[qemu_adaptation]]
