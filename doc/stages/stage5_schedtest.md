---
tags: [этап-5, планировщик, тесты, schedtest, механизм, шаг-0.2]
aliases: [schedtest, mechanism test]
stage: 5
---

# Организация `schedtest`

> [!abstract] Суть
> `schedtest` — отдельный режим бинарника (feature), который проверяет **механизм**: asm-переключение, стеки, сохранение контекста. Он не возвращает управление: забирает CPU вместо главного цикла и печатает вердикт последним. Организация: три статических стека с канарейками, две рабочие задачи с маркерами на стеке, монитор того же приоритета, счётчики на атомиках. Policy здесь не тестируется — она уже закрыта в `selftest` ([[stage5_policy_testing]]).

## Границы

| Проверяет | Не проверяет |
|---|---|
| `sched_switch`/`sched_start`, `sp`, `x19`–`x30` | правило выбора следующей задачи |
| целостность стеков и локальных переменных | события, `wait`/`wake` |
| переход `exit` без следующей задачи | вытеснение, квант |
| работу при включённых IRQ | тайминги (QEMU TCG) |

## Раскладка

```text
Cargo.toml            [features] selftest = [], schedtest = []
src/sched_switch.s    asm (подключается из scheduler.rs)
src/schedtest/mod.rs  статики, задачи, монитор, run()
src/main.rs           #[cfg(feature = "schedtest")] mod schedtest;
```

Один файл на ~130 строк — нормально. Разделять на `stacks.rs`/`tasks.rs` стоит, когда перевалит за ~150 строк; преждевременно дробить не нужно.

## Статики

```rust
const STACK_SIZE: usize = 16 * 1024;
const CANARY: u64 = 0xDEAD_BEEF_0001_0001;
const ROUNDS: u32 = 10;

#[repr(align(16))]
struct Stack([u8; STACK_SIZE]);

static mut STACK_A: Stack = Stack([0; STACK_SIZE]);
static mut STACK_B: Stack = Stack([0; STACK_SIZE]);
static mut STACK_MON: Stack = Stack([0; STACK_SIZE]);

static A_COUNT: AtomicU32 = AtomicU32::new(0);
static B_COUNT: AtomicU32 = AtomicU32::new(0);
static ERRORS: AtomicU32 = AtomicU32::new(0);
static A_DONE: AtomicBool = AtomicBool::new(false);
static B_DONE: AtomicBool = AtomicBool::new(false);
```

- Стеки — `static mut`, потому что запись идёт через `sp`; ссылок не берём, только `addr_of_mut!` (edition 2024).
- `[0; N]` → `.bss`, образ не растёт. Канарейка пишется в рантайме.
- Счётчики — атомики, а не `SpinLock`: тест проверяет переключение, а не синхронизацию. `Ordering::SeqCst` — чтобы не думать о переупорядочивании; цена на масштабе теста нулевая.
- Три стека, а не два: монитор — тоже задача и участвует в round-robin.

```rust
fn init_stack(stack: *mut Stack) -> usize {
    unsafe { (stack as *mut u64).write_volatile(CANARY) };
    stack as usize + core::mem::size_of::<Stack>()
}

fn canary_ok(stack: *mut Stack) -> bool {
    unsafe { (stack as *const u64).read_volatile() == CANARY }
}
```

## Рабочие задачи

```rust
extern "C" fn task_a() -> ! {
    let mut marker = [0xA5u8; 64];

    for _ in 0..ROUNDS {
        if marker.iter().any(|&b| b != 0xA5) {
            ERRORS.fetch_add(1, Ordering::SeqCst);
            marker = [0xA5; 64];
        }
        core::hint::black_box(&marker); // значение должно пережить yield
        A_COUNT.fetch_add(1, Ordering::SeqCst);
        scheduler::yield_now();
    }

    A_DONE.store(true, Ordering::SeqCst);
    scheduler::exit()
}
```

Почему маркер на стеке, а не только канарейка:

- `marker` живёт в кадре задачи. После `yield` его значение обязано сохраниться — это прямая проверка «стек задачи свой и кадр не затёрт».
- Канарейка внизу ловит **переполнение**, маркер — **перепутанные стеки/регистры**. Это разные ошибки.
- `black_box` не даёт оптимизатору выкинуть проверку.
- Печатать в цикле не нужно: печать — в мониторе, один раз. Логика «A/B видно глазами» — отдельный промежуточный чекпоинт.

`task_b` — то же самое с `0x5A`, `B_COUNT`, `B_DONE`.

## Монитор

```rust
extern "C" fn monitor() -> ! {
    while !(A_DONE.load(Ordering::SeqCst) && B_DONE.load(Ordering::SeqCst)) {
        scheduler::yield_now();
    }

    let counts = A_COUNT.load(Ordering::SeqCst) == ROUNDS
        && B_COUNT.load(Ordering::SeqCst) == ROUNDS;
    let errors = ERRORS.load(Ordering::SeqCst);
    let canaries = canary_ok(core::ptr::addr_of_mut!(STACK_A))
        && canary_ok(core::ptr::addr_of_mut!(STACK_B))
        && canary_ok(core::ptr::addr_of_mut!(STACK_MON));

    if counts && errors == 0 && canaries {
        crate::println!("sched: switch ok");
        crate::println!("sched: canary ok");
    } else {
        crate::println!(
            "sched: FAIL counts={} errors={} canary={}",
            counts, errors, canaries
        );
    }

    scheduler::exit()
}
```

> [!warning] Приоритет монитора
> Монитор должен быть **того же приоритета**, что A и B (или ниже). Если дать ему `Prio::DP` при строгом приоритете, он будет крутиться в `while … { yield }` и никогда не отдаст CPU рабочим задачам — тест «зависнет» без единой ошибки. Это первое, что стоит проверить, если вывод пуст.

Монитор печатает **один раз** и завершает тест. После его `exit()` готовых задач нет, `exit()` уходит в `wfi` — это заодно проверяет ветку «нет следующей задачи».

## Запуск

```rust
pub fn run() -> ! {
    let top_a = init_stack(core::ptr::addr_of_mut!(STACK_A));
    let top_b = init_stack(core::ptr::addr_of_mut!(STACK_B));
    let top_m = init_stack(core::ptr::addr_of_mut!(STACK_MON));

    scheduler::spawn(task_a, "a", Prio::CP, top_a).expect("schedtest: slot a");
    scheduler::spawn(task_b, "b", Prio::CP, top_b).expect("schedtest: slot b");
    scheduler::spawn(monitor, "mon", Prio::CP, top_m).expect("schedtest: slot mon");

    scheduler::run()
}
```

- `expect` уместен: если слотов не хватило — это ошибка конфигурации теста, а не штатный путь.
- Порядок `spawn` задаёт id, но тест не должен зависеть от id — только от счётчиков.
- `scheduler::run()` не возвращается.

## `main`

```rust
#[cfg(feature = "schedtest")]
mod schedtest;

// после gic/timer init и daifclr:
#[cfg(feature = "schedtest")]
{
    crate::println!("schedtest: start");
    schedtest::run();
}

#[cfg(not(feature = "schedtest"))]
loop {
    crate::println!("ticks = {}", timer::ticks());
    unsafe { core::arch::asm!("wfi") };
}
```

- `run()` возвращает `!`, поэтому цикл тиков обязан быть под `#[cfg(not(...))]`, иначе `unreachable_code`.
- Запуск **после** `daifclr` — намеренно: переключение должно работать с включёнными IRQ (таймер тикает, ISR планировщик не трогает).
- При обеих фичах `selftest` идёт раньше (он возвращается), затем `schedtest` (не возвращается).

## Контракт вывода

```text
schedtest: start
sched: switch ok
sched: canary ok
```

- Вердикт печатается **после** проверок.
- Паника/исключение → `PANIC: … at src/…:NN`, QEMU висит — вердикта нет.
- Автоматизация (QEMU сам не завершается):

```bash
timeout 60 cargo run --features schedtest 2>&1 | tee /tmp/schedtest.log
grep -q 'sched: canary ok' /tmp/schedtest.log && echo PASS || echo FAIL
```

## Проверка, что переключения действительно были

Счётчиков `A_COUNT`/`B_COUNT` **недостаточно**. Если `yield_now` вообще не переключает, каждая задача спокойно доделает свои 10 итераций и вызовет `exit()`, а `exit()` сам переведёт на следующую через `sched_start`. Монитор увидит `10/10` и напечатает `switch ok`.

Проверено экспериментально: в копии проекта закомментированы **все** `scheduler::yield_now()` — тест всё равно прошёл.

Лечится проверкой чередования:

```rust
static LAST: AtomicU32 = AtomicU32::new(u32::MAX); // MAX = «никого ещё не было»

fn note_run(id: u32) {
    let prev = LAST.swap(id, Ordering::SeqCst);
    if prev == id {
        // два раза подряд одна и та же задача => переключения не было
        ERRORS.fetch_add(1, Ordering::SeqCst);
    }
}
```

Вызов — в начале каждой итерации в `task_a` (`0`) и `task_b` (`1`); монитор отмечать не нужно. При честном round-robin `prev` всегда другая задача, при отсутствии переключений — та же.

После этого повторите эксперимент: с убранным `yield_now` должно быть `sched: FAIL … errors>0`. Это и есть CP4, только автоматический.

## Чекпоинты

| CP | Что включаем | Что видим |
|---|---|---|
| 1 | два бесконечных `A`/`B` с `println!` | `A B A B …` глазами |
| 2 | счётчики + монитор, без канарейки | `sched: switch ok` |
| 3 | маркеры на стеке + канарейки | `sched: canary ok` |
| 4 | негативная проверка | тест **падает** там, где должен |

CP4 — обязательный: тест, который никогда не падал, ничего не доказывает. Как проверить:

- Поставить в `spawn` `ctx.x30 = 0` → ожидаем «A» и дальше exception/тишину.
- Не записать канарейку → ожидаем `sched: FAIL … canary=false`.
- Сделать `STACK_A` размером 256 байт → ожидаем порчу/`FAIL`.

После каждой поломки — вернуть код. Это единственный способ убедиться, что проверки не декоративные.

## Ловушки

> [!warning]
> - **Монитор с высоким приоритетом** — вечный `yield` и мёртвый тест (см. выше).
> - **Печать из рабочих задач** в автоматическом режиме: много вывода и UART-lock в горячем цикле; вердикт теряется в шуме.
> - **`marker` без `black_box`** — оптимизатор вправе свернуть проверку.
> - **Счётчики `Relaxed`** — на одном ядре, скорее всего, сработает, но ошибка в модели памяти останется незамеченной; для теста берите `SeqCst`.
> - **`exit()` вызывается под удерживаемым lock'ом** — не возвращается и не отпускает; держите критические секции до `exit` закрытыми.
> - **Стек под `println!`**: форматирование использует заметный кадр; 16 КиБ — с запасом, 256 байт — нет.
> - **`STATIC_MUT` и ссылки**: `&mut STACK_A` в edition 2024 не компилируется; только `addr_of_mut!`.

## Что дальше

В 0.3 монитор уступит место idle-задаче, а проверки переедут в обычный цикл. В 0.4 в `schedtest` добавится сценарий «событие пришло до `wait`» и проснётся таймер. В workspace (фаза 1) статические стеки переедут в `firmware`/BSP, а сам `schedtest` станет отдельным бинарником или крейтом-тестом.

## Связанные заметки
- [[stage5_0_2_switch]], [[stage5_tdd]], [[stage5_policy_skeleton]], [[stage5_policy_testing]], [[context_switch]], [[no_std_testing]], [[roadmap_5_12]]
