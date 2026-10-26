# Bantou — браузер на чистом Rust с движком собственной разработки

> Blueprint и поэтапный план реализации. Стек: Slint (UI, Material 3) + tokio (async) +
> reqwest (сеть) + html5ever (парсер HTML) + собственный layout/paint поверх Slint.

## 1. Системная архитектура

```
┌──────────────────────────  UI (главный поток)  ──────────────────────────┐
│  Slint BrowserWindow  ── M3 Expressive оболочка: табы, URL-бар, сайдбар  │
│  Свойства (page-html, tab-list, progress) + callbacks (navigate, …)      │
└──────────────△───────────────────────────────────────────────△───────────┘
               │ invoke_from_event_loop (обновление UI)         │ callback
┌──────────────┴─────────────────  Engine (tokio runtime)  ──┴───────────┐
│                                                                        │
│  1. NET      src/engine/net.rs      reqwest + rustls, пул соединений,  │
│              параллельная загрузка ресурсов (subfetch), таймауты        │
│  2. PARSE    src/engine/html.rs     html5ever → RcDom → свой DomNode   │
│              (дерево с инлайн-стилями и классами)                      │
│  3. CSS      src/engine/style.rs   (фаза 3) мини-парсер CSS: селекторы  │
│              тег/класс/id, каскад, наследование font/color             │
│  4. LAYOUT   src/engine/layout.rs   бокс-модель: block/inline, отступы, │
│              перенос строк, ширина контента → Vec<LaidOutBox>           │
│  5. PAINT    фаза 3: список примитивов (текст, прямоугольники,        │
│              изображения) → слои Slint (Text/Rectangle) или на Canvas  │
│                                                                        │
└────────────────────────────────────────────────────────────────────────┘
```

Поток данных: `navigate(url)` → поток в пуле tokio → `net::fetch` → `html::parse` →
`layout::layout(dom, viewport_width)` → `invoke_from_event_loop` → UI рендерит боксы.
Ничего блокирующего в потоке UI нет; тяжёлые стадии (layout больших страниц) —
`tokio::task::spawn_blocking`.

### Ключевые типы (фаза 1–2)

```rust
pub struct DomNode {                    // упрощённое DOM-дерево
    pub tag: String,                    // "" у текстовых узлов
    pub attrs: HashMap<String, String>,
    pub children: Vec<DomNode>,
    pub text: String,                   // только у текстовых узлов
}

pub struct LaidOutBox {                 // результат layout'а
    pub x: f32, pub y: f32,
    pub width: f32, pub height: f32,
    pub kind: BoxKind,                  // Block | InlineText | Image | Rule
    pub content: BoxContent,            // Text(String, FontSpec) | ...
}
```

## 2. Дизайн-система Material 3 Expressive → Slint

Все токены собраны в `ui/theme.slint` как `global Theme`. Слайдер-пресеты
(`styles.slint`) сгенерированы Slint из базового цвета `#6750A4`.

### Цветовые роли (M3)

| Роль | Токен | Светлая |
|---|---|---|
| primary / on-primary | `Theme.primary` | `#6750A4` / `#FFFFFF` |
| primary-container | `Theme.primary-container` | `#EADDFF` |
| surface | `Theme.surface` | `#FEF7FF` |
| surface-container-low / -high | | `#F3EDF7` / `#ECE6F0` |
| outline / outline-variant | | `#79747E` / `#CAC4D0` |
| error / on-error | | `#B3261E` / `#FFFFFF` |

### Формы (shape scale, Expressive)

| Токен | Значение | Применение |
|---|---|---|
| `shape-corner-xs` | 4px | чипы, бейджи |
| `shape-corner-s` | 8px | карточки |
| `shape-corner-m` | 12px | поля ввода, меню |
| `shape-corner-l` | 16px | карточки Expressive |
| `shape-corner-xl` | 28px | FAB, табы |
| `shape-corner-full` | 999px | поиск-бар, переключатели |

### Типографика

| Стиль | Размер / вес | Применение |
|---|---|---|
| display-small | 36px / 400 | заголовок пустой страницы |
| headline-small | 24px / 400 | заголовок страницы |
| title-medium | 16px / 500 | заголовки вкладок |
| body-medium | 14px / 400 | основной текст |
| label-large | 14px / 500 | кнопки |

### Анимация (motion, физика пружин M3 Expressive)

| Токен | Кривая | Длительность | Применение |
|---|---|---|---|
| `emphasized` | cubic-bezier(0.2, 0, 0, 1) | 500ms | появление сайдбара, морфинг FAB |
| `emphasized-decelerate` | cubic-bezier(0.05, 0.7, 0.1, 1) | 400ms | въезд контента |
| `emphasized-accelerate` | cubic-bezier(0.3, 0, 0.8, 0.15) | 200ms | выезд/скрытие |
| `standard` | cubic-bezier(0.2, 0, 0, 1) | 250ms | ховеры, вкладки |
| spring-quick | затухающая пружина (bounce) | ~350ms | тактильный отклик кнопок |

Пружинные анимации Slint 1.8+ (`animate` с `spring()`) используются там, где нужен
отскок; кривые `ease_out_*` — для остальных переходов.

### Компоненты-ориентиры M3

- **Top app bar** — строка вкладок + URL-бар (форма `full`, высота 56px).
- **Navigation rail** — вертикальный сайдбар (избранное/история), ширина 80px ↔ 260px
  с морфингом по `emphasized`.
- **Extended FAB** — «новая вкладка» (иконка + текст, радиус 28px, морфинг при наведении).
- **Linear progress indicator** — волнообразный индикатор загрузки (Expressive).
- **Chip / button group** — переключение режима поиска.

## 3. Структура проекта и код

```
bantou/
├── Cargo.toml
├── build.rs                  # slint_build::compile (SLINT_STYLE=material)
├── ui/
│   ├── theme.slint           # токены M3 Expressive (цвет, форма, шрифт, движение)
│   ├── browser.slint         # оболочка окна
│   └── (фаза 3) page-view.slint — рендер LaidOutBox
└── src/
    ├── main.rs               # точка входа, мост Slint ⇄ tokio
    ├── lib.rs                # корень движка
    └── engine/
        ├── mod.rs
        ├── net.rs            # фаза 1 — fetch
        ├── html.rs           # фаза 1 — парсинг в DomNode
        ├── style.rs          # фаза 3 — CSS
        └── layout.rs         # фаза 2 — бокс-модель
```

## 4. Фазы реализации (прототип → рабочий браузер)

**Фаза 1 — рабочий скелет (готово).** Окно M3, URL-бар → Google-поиск по умолчанию,
fetch через reqwest (rustls), парсинг html5ever, текст страницы в ScrollView.

**Фаза 2 — читаемый рендер (текущая).** `layout.rs`: блоки/инлайн-текст, отступы
заголовков h1–h6 и p, перенос по словам по ширине окна, `LaidOutBox` → простой
пейнтер в Slint. Табы: несколько вкладок с состоянием (загрузка/готово), spring-анимация
переключения, сайдбар с морфингом.

**Фаза 3 — CSS и честный paint.** Мини-парсер CSS (тег/класс/id, специфичность),
бокс-модель с margin/padding/border, инлайн-поток со шрифтами разного размера,
рендер в `Rectangle`/`Text` слои Slint или в канвас (femtovg). Изображения — reqwest →
`SharedPixelBuffer`.

**Фаза 4 — интерактивность.** Дерево хитов по `LaidOutBox`, клики по ссылкам,
формы, история + закладки (SQLite/sled), JS — опционально (bootstrap: `boa_engine`).

**Фаза 5 — производительность.** Кэш HTTP, параллельная загрузка ресурсов,
инкрементальный relayout (только изменённые поддеревья), рендер-кэш слоёв,
профилирование `tracing` + `tokio-console`.

## 5. Сборка на этой машине (важно)

Тулчейн Rust — `x86_64-pc-windows-msvc`, но MSVC Build Tools **не установлены**
(каталог VS 2022 пуст, в PATH попадает GNU `link.exe` из Git Bash — он не годится).
Без согласия пользователя ничего не устанавливалось. Варианты, когда будет дано согласие:

1. **MSVC (рекомендуется)** — поставить Visual Studio Build Tools (workload
   «Desktop development with C++»), далее просто `cargo build`.
2. **Без Visual Studio** — `rustup target add x86_64-pc-windows-gnu` и в
   `.cargo/config.toml` указать `target = "x86_64-pc-windows-gnu"`; у self-contained
   GNU-тулчейна свой линковщик, VS не нужен.

Пока код проверен только статически; слэбовская компиляция UI заработает после
устранения линковщика.

## 6. Справочники

- M3 Expressive: https://m3.material.io/foundations/expressive-design/overview (кривые
  `emphasized*`, спринг-механика, shape scale до 30 значений, морфинг FAB).
- Slint: https://docs.slint.dev — декларативные `.slint`-файлы, биндинги свойств,
  `animate`/`spring`, бэкенды winit+femtovg/skia, `invoke_from_event_loop`.
- Servo (референс архитектуры): https://github.com/servo/servo — pipeline
  Script → Layout → Paint, разделение потоков.
