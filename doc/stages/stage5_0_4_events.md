---
tags: [этап-5, планировщик, события, irq, атомики, шаг-0.4]
aliases: [0.4, events, wake_from_isr, wait, set_waker]
stage: 5
---

# Шаг 0.4: события и пробуждение задач

> [!abstract] Суть
> ISR больше не трогает планировщик: он только выставляет биты в атомиках (`PENDING[id]`, `READY`) и берёт адресата из реестра `WAKERS[event]`. Планировщик сливает накопленное в policy на safe point (`yield_now`, `wait_current`). Тики учитываются **счётчиком**, а бит `TICK` служит только сигналом «есть работа»: так ни один тик не теряется. Двухфазный `block` отложен до этапа 6.

## Принятые решения (2026-11-06)

1. **`TICK` — счётчик, а не только бит.** Бит нужен для пробуждения, счётчик — чтобы не терять «лишние» тики между drain'ами.
2. **Реестр источников `WAKERS[event]`.** `set_waker(event, id)` вместо одного `TICK_WAKER`: таймер, NIC, TX-завершение добавляются одинаково.
3. **Двухфазный block отложен до этапа 6.** Сейчас остаётся узкое окно между `block_current` и переключением; оно смягчается порядком `yield → wfi` в idle и не критично для таймера.

## Границы шага

| Входит | Не входит |
|---|---|
| `PENDING`/`READY`, `wake_from_isr`, drain | NIC как источник (этап 6) |
| реестр `WAKERS` и `set_waker` | несколько ожидающих на одно событие |
| `wait_current(mask)` | вытеснение по таймеру (5.5, опционально) |
| счётчик тиков + задача-тикер | дедлайны и `CNTV_CVAL` (0.5) |
| idle без печати | двухфазный block (этап 6) |

## Данные

```rust
// scheduler.rs
static PENDING: [AtomicU32; MAX_TASKS] = [const { AtomicU32::new(0) }; MAX_TASKS];
static READY: AtomicU32 = AtomicU32::new(0);                 // бит i = есть события для i
static WAKERS: [AtomicU32; 32] = [const { AtomicU32::new(u32::MAX) }; 32];
```

```rust
// events.rs
#![allow(dead_code)] // TODO(этап 6): NET_RX/TX_DONE понадобятся драйверу

pub type EventSet = u32;

pub const TICK: EventSet = 1 << 0;
pub const NET_RX: EventSet = 1 << 1;
pub const TX_DONE: EventSet = 1 << 2;

pub fn bit_index(ev: EventSet) -> usize {
    debug_assert!(ev.is_power_of_two(), "event must be a single bit");
    ev.trailing_zeros() as usize
}
```

`EventSet` переезжает из `scheduler.rs` в `events.rs`; в планировщике — `use crate::events::EventSet;`. Тесты его не импортируют.

Почему атомики отдельно от `Scheduler`:

- ISR не берёт `&mut SCHED` — нет гонки с задачей и deadlock на локе;
- policy-структура остаётся чистой и тестируемой;
- drain — единственное место, где атомики превращаются в состояния задач.

## ISR: два атомика и реестр

```rust
pub fn wake_from_isr(id: usize, bits: EventSet) {
    if id >= MAX_TASKS || bits == 0 {
        return;
    }
    PENDING[id].fetch_or(bits, Ordering::Release);
    READY.fetch_or(1u32 << id, Ordering::Release);
}

pub fn set_waker(ev: EventSet, id: usize) {
    debug_assert!(id < MAX_TASKS);
    let idx = events::bit_index(ev);
    if idx < WAKERS.len() {
        WAKERS[idx].store(id as u32, Ordering::Release);
    }
}

/// Вызывается из ISR: сам находит адресата по событию.
pub fn wake_from_event(ev: EventSet) {
    let idx = events::bit_index(ev);
    if idx >= WAKERS.len() {
        return;
    }
    let id = WAKERS[idx].load(Ordering::Acquire);
    if (id as usize) < MAX_TASKS {
        wake_from_isr(id as usize, ev);
    }
}
```

- Порядок записи строгий: сначала `PENDING`, потом `READY`. Иначе возможен «проснулись, а битов нет».
- `Release`/`Acquire`: на одном ядре хватило бы `Relaxed`, но паттерн должен остаться верным при SMP.
- **Один waker на событие.** Для таймера этого достаточно; если на `NET_RX` будут ждать несколько задач, реестр станет битмапой задач (этап 6).

## Счётчик тиков

Бит и счётчик отвечают на разные вопросы:

| Вопрос | Что отвечает |
|---|---|
| «есть ли работа?» | бит `TICK` в `PENDING` |
| «сколько тиков прошло?» | `timer::ticks()` |

```rust
// timer.rs
static TICKS: AtomicU64 = AtomicU64::new(0);

pub fn ticks() -> u64 {
    TICKS.load(Ordering::Acquire)
}

pub fn on_irq() {
    TICKS.fetch_add(1, Ordering::Release);           // сначала количество
    crate::scheduler::wake_from_event(crate::events::TICK); // потом сигнал
    // … переустановить CNTV_TVAL …
}
```

Почему так: три тика до drain дадут **один** бит в `PENDING` и `+3` в счётчике. Задача проснётся один раз и обработает дельту — ни один тик не потерян.

> [!warning] Счётчик — не часть `wait_mask`
> `wait_current(TICK)` возвращает бит `TICK`, а не количество. Количество читается отдельно из `timer::ticks()`. Смешивать эти две вещи в одном `EventSet` нельзя: битовая маска не умеет считать.

## Drain (safe point)

```rust
impl Scheduler {
    fn drain_irq(&mut self) {
        let mut ready = READY.swap(0, Ordering::Acquire);
        while ready != 0 {
            let id = ready.trailing_zeros() as usize;
            ready &= ready - 1;

            let bits = PENDING[id].swap(0, Ordering::Acquire);
            if bits != 0 {
                self.wake(id, bits);
            }
        }
    }
}
```

- `swap(0)` забирает накопленное; пришедшее после останется на следующий drain.
- `wake` — существующий policy-метод: латчит биты в `Task.pending`, переводит `Blocked → Ready` только при совпадении с `wait_mask`.
- Вызывать **только** из контекста задачи, никогда из ISR.

Где: в начале `yield_now`, в `wait_current` перед каждой проверкой, и в idle перед `wfi`.

> [!warning] Симптом забытого drain в `yield_now`
> Тикер спит, idle крутит `yield_now → wfi`, IRQ исправно ставит биты в `PENDING`/`READY`, но никто их не забирает. Снаружи это выглядит как «зависание после `VBAR`»: паники нет, тиков нет, хотя таймер тикает. Проверяйте первым делом, что `drain_irq` есть **в `yield_now`**, а не только в `wait_current`.

## `wait_current`

```rust
pub fn wait_current(mask: EventSet) -> EventSet {
    loop {
        let (me, next) = {
            let sched = unsafe { &mut *core::ptr::addr_of_mut!(SCHED) };
            sched.drain_irq();

            if let Some(ev) = sched.try_wait_current(mask) {
                return ev;
            }

            let me = sched.current().expect("wait without current");
            match sched.block_current(mask) {
                BlockResult::Blocked { next } => (me, next),
                BlockResult::NotBlocked => continue,
            }
        };

        match next {
            Some(next) => switch_between(me, next),
            None => halt(),
        }
        // при возобновлении: снова drain + try_wait
    }
}
```

`switch_between(from, to)` — тот же asm, что в `yield_now`; вынести в общую функцию:

```rust
fn switch_between(from: usize, to: usize) {
    let sched = unsafe { &mut *core::ptr::addr_of_mut!(SCHED) };
    let base = sched.tasks.as_mut_ptr();
    let from_ctx = unsafe { core::ptr::addr_of_mut!((*base.add(from)).ctx) };
    let to_ctx = unsafe { core::ptr::addr_of!((*base.add(to)).ctx) };
    unsafe { sched_switch(from_ctx, to_ctx) };
}
```

### Разбор гонок

| Когда пришло событие | Что происходит |
|---|---|
| до `try_wait` | drain положил биты в `Task.pending` → немедленный возврат |
| между `try_wait` и `block_current` | policy пуст → сон принят; биты в атомике; следующий safe point разбудит |
| после `block_current`, до `switch_between` | биты в атомике, задача `Blocked`; разбудит первый drain у следующей задачи |
| пока задача `Running` | drain положит биты в `Task.pending`; `wait` вернётся сразу |

Событие не теряется ни в одном случае; задержка — до следующего safe point.

> [!warning] Остаточное окно (осознанно, до этапа 6)
> Если событие пришло между `block_current` и переключением в idle, а idle сразу ушёл в `wfi`, пробуждение отложится до следующего IRQ (250 мс). Смягчение — порядок в idle:
>
> ```rust
> loop {
>     scheduler::yield_now(); // drain до сна
>     unsafe { core::arch::asm!("wfi") };
> }
> ```
>
> Полное закрытие — двухфазный block (отмена переключения), запланирован на этап 6, когда появится NIC и задержка станет важной.

## Таймер как источник

```rust
// timer::on_irq
TICKS.fetch_add(1, Ordering::Release);
crate::scheduler::wake_from_event(crate::events::TICK);
```

Тикер-задача (CP):

```rust
extern "C" fn ticker() -> ! {
    let mut last = timer::ticks();

    loop {
        scheduler::wait_current(events::TICK);

        let now = timer::ticks();
        let delta = now - last;
        last = now;

        println!("ticks = {} (+{})", now, delta);
    }
}
```

`+delta` — дешёвая проверка, что счётчик работает: в норме `+1`, при задержке больше одного тика видно `+2`, `+3`.

Idle теряет печать:

```rust
pub extern "C" fn task() -> ! {
    loop {
        scheduler::yield_now();
        unsafe { core::arch::asm!("wfi") };
    }
}
```

## Порядок в `main`

Меняются три вещи:

1. появляются `mod events;` и `mod ticker;`;
2. после idle спавнится тикер и регистрируется `scheduler::set_waker(events::TICK, tick)`;
3. `msr daifclr` переезжает из блока инициализации GIC вниз — после всех `spawn`/`set_waker`, но до `run()`.

Всё остальное (печать адресов, MMU, `irq::init`, heap, selftest) не меняется.

```rust
gic::GIC.init();
gic::GIC.enable_ppi(timer::INTID, timer::PRIORITY);
timer::init();
// IRQ ещё замаскированы (daifset в _start)

let idle_top = &raw const stack_top as usize;
scheduler::spawn(idle::task, "idle", scheduler::Prio::IDLE, idle_top).expect("idle slot");

let tick = scheduler::spawn(ticker, "tick", scheduler::Prio::CP, ticker_top).expect("tick slot");
scheduler::set_waker(events::TICK, tick);   // до daifclr!

#[cfg(feature = "schedtest")]
schedtest::spawn_tasks();

unsafe { core::arch::asm!("msr daifclr, #2", "isb") };
scheduler::run();
```

> [!warning] Регистрация до включения IRQ
> Если включить IRQ раньше `set_waker`, первые тики придут без адресата и будут отброшены как события (счётчик их сохранит, но задача не проснётся до следующего). Порядок `spawn → set_waker → daifclr` убирает этот класс сюрпризов.

### Стек тикера

Загрузочный стек уже занят idle, поэтому тикеру нужен свой:

```rust
// ticker.rs
const STACK_SIZE: usize = 8 * 1024;

#[repr(align(16))]
struct Stack([u8; STACK_SIZE]);

static mut STACK: Stack = Stack([0; STACK_SIZE]);

pub fn stack_top() -> usize {
    core::ptr::addr_of_mut!(STACK) as usize + core::mem::size_of::<Stack>()
}
```

`Stack.0` снова даст `dead_code` (поле читается только через адрес). Два варианта: `#[allow(dead_code)]` как в `schedtest`, либо без структуры:

```rust
static mut STACK: [u128; STACK_SIZE / 16] = [0; STACK_SIZE / 16];
```

У `u128` выравнивание 16, поля нет — предупреждения тоже.

## Тесты

Policy (selftest):

| Сценарий | Ожидание |
|---|---|
| `wake_from_isr` + `drain_irq` | `Task.pending` содержит биты |
| drain для `Blocked` с совпадающей маской | `Ready` |
| drain для `Unused`/`Exited` | биты отброшены, паники нет |
| два `wake_from_isr` до drain | один бит в `pending`, один `Ready` |
| `set_waker(TICK, id)` + `wake_from_event(TICK)` | `PENDING[id]` содержит `TICK` |
| `wake_from_event` без waker | ничего не выставлено |

QEMU:

| Сценарий | Ожидание |
|---|---|
| обычный запуск | `ticks = N (+1)` ~4/с из тикера |
| `wake_from_isr` до `wait` | `wait` возвращается сразу, без переключения |
| `wait` на `NET_RX`, таймер шлёт `TICK` | задача не просыпается |
| `set_waker` не вызван | тиков нет (счётчик растёт, но сигнала нет) |
| `schedtest` | как раньше, события не использует |

Проверка счётчика «на потерю»: временно задержать тикер (заняться работой на 2–3 тика) и увидеть `+2`/`+3`, затем возврат к `+1`.

## DoD

- тикер печатает `ticks = N (+1)` ~4/с, idle не печатает;
- `wait` на неподходящую маску не просыпается;
- в ISR нет локов/печати/аллокаций;
- clippy чист в четырёх конфигурациях, `fmt` чист;
- `TODO(0.4)` у модульного allow пересмотрен.

## Подводные камни

> [!warning]
> - `EventSet` — **только сигналы**, количество живёт в счётчике. Не пытайтесь «положить 3 в `PENDING`».
> - `wake_from_isr` обязан проверять `id < MAX_TASKS`: реестр может содержать `u32::MAX`.
> - `set_waker` принимает только однобитное событие; `bit_index` это проверяет.
> - Не вызывать `drain_irq` из ISR — она берёт `&mut SCHED`.
> - `READY.fetch_or` без `PENDING` даст «проснулись, а битов нет»; порядок строгий.
> - `switch_between(me, next)` — только после того, как ссылка на `SCHED` умерла; `me` запомнить **до** `block_current`.
> - `wfi` без включённых IRQ — не сон, а спин.
> - Регистр `WAKERS` живёт между перезапусками задачи: если задача завершилась, waker останется указывать на `Exited`-слот, и `wake_from_isr` просто отбросит биты. Это безопасно, но лучше явно снимать waker при `exit`.

## Открытые вопросы

> [!question]
> - Нужен ли `clear_waker(event)` при завершении задачи, или отбрасывания битов достаточно?
> - Несколько ожидающих на одно событие (битмапа задач вместо одного id) — этап 6?
> - `TICKS` как `u64` — оставляем; переполнение при 4 Гц недостижимо.
> - Пора ли вынести реестр и атомики в отдельный модуль `events` целиком (сейчас `events` — только константы)?

## Связанные заметки
- [[stage5_0_3_idle]], [[stage5_0_2_switch]], [[stage5_schedtest]], [[0009_hybrid_scheduling]], [[irq_entry_exit]], [[atomic_counter]], [[memory_barriers]], [[roadmap_5_12]]
