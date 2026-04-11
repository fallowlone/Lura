# PROGRESS.md — doc-format project

**Обновлено:** 11 апреля 2026

---

## Текущая фаза: Render Engine v2 (Rust)

**Статус:** активная разработка

---

## Стек

- Язык: Rust
- Режим: Claude Code пишет код, Artem архитектурит и ревьюит

---

## Format specification — прогресс

- [x] Базовый синтаксис определён на бумаге
- [x] Типы блоков: heading, paragraph, table, figure, code (`H1`–`H6`, `P`, `TABLE`/`ROW`/`CELL`, `FIGURE`/`IMAGE`, `CODE`; PDF: placeholder для пустого asset)
- [x] Layout: grid-based (не coordinate-based)
- [x] Block ID схема
- [x] Certificate схема
- [x] Asset handling: inline (base64) vs linked (hash)

### Синтаксис (черновик)

```
STYLES({
  #mainColor: #FF0000
  #mainFont: "Arial"
})

PAGE(
  STYLES({
    #bgColor: #FFFFFF
  })

  H1({color: #mainColor} Hello World)

  P(Текст параграфа)

  GRID({columns: "1fr 2fr"}
    P(Левая колонка)
    P(Правая колонка)
  )
)
```

Правила:

- Блок: `TYPE({атрибуты} контент)` или `TYPE(контент)`
- Атрибуты опциональны
- STYLES всегда первый блок (документ и страница)
- Переменные: `#name`, доступны везде (два прохода парсера)
- Grid: columns = фиксированные / пропорции (fr) / auto

---

## Renderer — прогресс

- [x] AST → JSON
- [x] AST → plain text
- [x] AST → HTML
- [x] Engine v2: StyledTree -> LayoutTree -> PageTree -> PDF (`pdf-writer`)
- [x] Удалён legacy PDF путь на `printpdf`

## Lexer — прогресс

- [x] Определены токены
- [x] Написан базовый Lexer (mode-based: Normal / Attrs / Content)
- [x] Тесты для всех типов токенов

## Parser — прогресс

- [x] AST определён (Document, Block, Content, Value)
- [x] Базовый Parser: токены → AST
- [x] Переменные: подстановка #var в атрибутах (два прохода)
- [x] Тесты
- [x] AST переведён на arena-модель (`NodeId`, `Content::Children`)
- [x] `id::assign_ids` переведён на post-order обход по ID
- [x] API AST очищен: внешние модули используют методы `Document`, а не внутренние поля arena
- [x] Inline AST v1: `TextRun`, `Emphasis`, `Strong`, `CodeSpan`, `LinkSpan`

---

## Engine v2 — прогресс

- [x] Data-oriented Styled Arena (`id-arena`)
- [x] Интеграция `taffy` для layout (Grid/Flex)
- [x] Пагинация `LayoutTree -> PageTree` (A4)
- [x] `unicode-linebreak` для переносов
- [x] `fontdb` для системных шрифтов
- [x] `rustybuzz` shaping для измерения текста
- [x] PDF backend на `pdf-writer`
- [x] Каркас Painter API
- [x] WGPU backend scaffold под feature `wgpu-preview` (stub)
- [x] Inline layout v1: line builder по run-ам + mixed-style rendering (PDF/SVG)
- [x] Typography v1: `letter-spacing`, `word-spacing`, базовый `justify`
- [x] Pagination rules v2 (base): `keep-with-next`, `keep-together`, row split policy switch
- [x] Global deps foundation: multi-pass convergence guard + `counters`/`introspection` модули
- [x] Advanced layout foundation: min/max constraints, float mode (left/right), page header/footer
- [x] Export parity quality gates: capability matrix + integration smoke tests + cache regression test

---

## Открытые вопросы

_(нет)_

---

## Решения принятые

- Язык реализации: Rust
- Семантические блоки вместо визуальных координат
- Diff-friendly: стабильные block ID
- Два режима ассетов: self-contained (base64) и linked (external + hash)
- Верификация без центрального CA — самодостаточная
- Синтаксис формата: human-readable текстовый (не JSON/YAML)
- Sparse layout: координаты в абсолютных единицах (mm)
- Certificate: SHA-256 хеш всего документа
- Folio = формат хранения; редактор и authoring syntax — отдельные проекты поверх
