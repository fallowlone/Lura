# Lura UI Overhaul, File-Open Fix, and Performance Pass

**Date:** 2026-04-17
**Status:** Design approved, awaiting user spec review
**Approach:** Phased — three independent ship-ready slices

## Problem

Three pain points in the current Lura macOS host app:

1. **File-open broken.** Double-clicking a `.lura` file in Finder launches the app but lands on the Welcome screen — the file URL is never routed to the editor. The user must then use the Open dialog, which triggers a macOS TCC permission prompt for the containing folder.
2. **Permission prompt repeats.** Files clicked from the Recent list re-prompt for access after restart because raw `URL` values are persisted instead of security-scoped bookmarks.
3. **UI feels generic and flat.** Welcome is reasonable but Editor is a plain `TextEditor` with mono font, no syntax highlight, no line numbers. Toolbar is utilitarian. No app icon. No coherent visual identity.
4. **General performance feels sluggish** — no measured baseline today.

## Goals

- Double-click in Finder opens the file directly in the editor.
- Recents reopen across reboot without any permission prompt.
- App has a coherent Pages/Bear-inspired warm-paper aesthetic, light + dark, with a custom Lura editor and proper app icon.
- App feels snappy: cold launch, file open, typing, and preview all measured and within target budgets.

## Non-goals

- Visual editing of Lura blocks (still source-text editing).
- Cloud sync, collaboration, plugin system.
- Replacing the Rust core or PDF pipeline.
- App Store submission (sandbox stays for security, not distribution).

## Approach: Phased delivery

Three slices, each independently shippable. Each unblocks the next without coupling the work.

```
Phase A (1-2 d)  →  Phase B (3-5 d)  →  Phase C (2-3 d)
file-open + bookmarks  →  full UI overhaul  →  measured perf pass
```

---

## Phase A — File-open and bookmarks

**Outcome:** Finder double-click opens editor. Recents reopen without prompt across restart.

### Components touched

- `quicklook/HostApp/LuraApp.swift` — add `.handlesExternalEvents(preferring:allowing:)` and `.onOpenURL { url in appModel.openDocumentURL(url) }` on `WindowGroup`.
- `quicklook/HostApp/LuraAppDelegate.swift` — implement `application(_:open urls:)` as the cold-launch fallback for cases where the SwiftUI scene is not yet attached. Maintain a `pendingURLs: [URL]` queue, flushed when the model registers itself.
- `quicklook/HostApp/RecentFilesStore.swift` — change persisted shape from `[URL]` to `[RecentEntry]` where:
  ```swift
  struct RecentEntry: Codable {
      var bookmark: Data        // .withSecurityScope
      var displayPath: String
      var lastOpened: Date
  }
  ```
  Bookmarks created via `URL.bookmarkData(options: .withSecurityScope, includingResourceValuesForKeys: nil, relativeTo: nil)`.
- `quicklook/HostApp/LuraAppModel.swift` — `openDocumentURL(_:)` gains an internal helper that resolves bookmark data, calls `startAccessingSecurityScopedResource()`, and tracks the live URL on the document.
- `quicklook/HostApp/LuraFileDocument.swift` — store the active `URL`, call `stopAccessingSecurityScopedResource()` in `deinit`. Add a flag so we only stop scopes we ourselves started (Powerbox-granted URLs already have system-managed scope).

### Data flow

Finder double-click (cold launch):

```
Finder → LaunchServices → app launch
  → AppDelegate.application(_:open urls:) fires
  → URLs queued on AppDelegate
  → LuraAppModel registers, drains queue
  → openDocumentURL(url) → save bookmark → openEditorURL = url
  → RootView shows LuraEditorContainer
```

Finder double-click (warm):

```
Finder → already-running app
  → WindowGroup.onOpenURL fires
  → openDocumentURL(url)
```

Recents click (after restart):

```
RecentRow tap → RecentFilesStore resolves bookmark → live URL
  → startAccessingSecurityScopedResource()
  → openDocumentURL(liveURL)
  → on document deinit → stopAccessingSecurityScopedResource()
```

### Error handling

- Bookmark `isStale == true` → silently re-create from the resolved URL.
- Bookmark resolution fails → inline alert "File moved or deleted", offer "Remove from Recents".
- Cold-launch URL arrives before model ready → AppDelegate queue handles it.
- Same file already open and dirty → existing `mayReplaceOpenDocument()` confirmation flow.

### Testing

- Manual: Finder double-click cold + warm; Recents reopen after `killall Lura` and reboot; file moved between sessions.
- Unit: `RecentFilesStore` bookmark roundtrip; stale-bookmark refresh; queue drain ordering.

### Estimate

1–2 days.

---

## Phase B — UI overhaul

**Outcome:** Pages/Bear warm-paper aesthetic, custom NSTextView-based editor with syntax highlight, line numbers, fold, minimap, asset thumbnails, new app icon, light + dark.

### Design tokens

New file `quicklook/HostApp/LuraTheme.swift`. Single source for color, typography, spacing.

**Palette light**

| Token  | Hex       |
| ------ | --------- |
| paper  | `#FAF6EE` |
| ink    | `#2B2418` |
| muted  | `#8C8273` |
| accent | `#A0522D` (sienna) |
| rule   | `#E5DCC8` |

**Palette dark**

| Token  | Hex       |
| ------ | --------- |
| panel  | `#1F1B17` |
| ink    | `#EFE6D4` |
| muted  | `#8B8273` |
| accent | `#D9925E` |
| rule   | `#3A3328` |

**Typography**

- Body: `New York` (system serif).
- Mono / code: `JetBrains Mono` (OFL 1.1, redistribution allowed) bundled in `quicklook/Resources/Fonts/`, fallback `SF Mono`.
- Heading scale: 32 / 24 / 18 / 15 pt.

**Spacing & shape**

- 4-pt grid (4, 8, 12, 16, 24, 32).
- Corner radii: 8, 12, 16.
- Soft shadow: `y=2 blur=12 alpha=0.06`.

Exposed via `@Environment(\.luraTheme)`. Switches automatically on `colorScheme`.

### Components

1. **`LuraTheme`** — environment value with `colors`, `fonts`, `spacing`, `radii`. Single `EnvironmentKey`.
2. **`WelcomeView` refresh**
   - Replace gradient background with paper texture (subtle SVG noise overlay at 4% opacity).
   - Serif title using New York 34pt.
   - Action cards become "letterhead" cards: hairline rule top, monogram glyph, serif title, mono subtitle.
   - Recents list shows file thumbnail (Quick Look thumbnail of the document's first PDF page).
3. **`LuraEditor`** — new file `quicklook/HostApp/LuraEditor.swift`. `NSViewRepresentable` wrapping `NSTextView` inside `NSScrollView`. Replaces the `TextEditor` in `LuraEditorView`.
   - **Line numbers:** `LineNumberRulerView: NSRulerView` on the left edge.
   - **Syntax highlight:** `LuraSyntaxHighlighter` using TextKit 2 `NSTextContentManager`. Tokenizer is regex-based for v1: headings (`^#{1,6} `), code fences (` ``` … ``` `), inline code (`` ` … ` ``), links `[text](url)`, asset embeds `![alt](path)`, list markers, blockquotes. Tokens map to color from `LuraTheme.colors`.
   - **Current-line highlight:** subtle paper-warm tint behind the active line.
   - **Asset thumbnails:** when a `![alt](path)` token is parsed and the file resolves, the text range is replaced with an `NSTextAttachment` rendering a 40-pt-tall downscaled image inline. Editing the embed text restores the textual representation.
   - **Fold ribbon:** gutter strip between line numbers and text. Click a triangle on a heading or code-fence start line to collapse to a single placeholder line. Fold state held per document URL in memory only (not persisted in v1).
   - **Minimap:** `NSView` on the right edge (80 pt wide). Paints each line as a colored rectangle by token category. Draggable viewport indicator. Toggle in toolbar.
4. **`PDFPreviewRepresentable` refresh**
   - Paper-frame border using `LuraTheme.colors.rule`.
   - Page indicator chip in upper-right showing `n / total`.
   - Floating zoom slider pill bottom-right.
   - Default to fit-width.
5. **Toolbar** — single unified row using `.windowToolbarStyle(.unified)`. Layout: `[traffic-lights] · [serif doc title centered] · [Save] [Open] [Preview toggle] [Minimap toggle]`.
6. **App icon** — design new icon: paper sheet with serif "L" monogram and a sienna corner fold (callback to the warm-paper aesthetic).
   - Source SVG in `quicklook/Resources/icon.svg`.
   - Generation script `scripts/build-icon.sh` produces 16 / 32 / 64 / 128 / 256 / 512 / 1024 + @2x via `iconutil`.
   - Wired into `quicklook/HostInfo.plist` as `CFBundleIconFile`.
7. **Light/dark** — `LuraTheme` palettes swap via `@Environment(\.colorScheme)`. `NSTextView` text attributes are recomputed on color-scheme change via a `Combine` pipeline that observes `NSApp.effectiveAppearance`.

### Data flow

```
document.text changes
  → LuraSyntaxHighlighter.tokenize(changedRange)
      (incremental, only the dirty paragraph range)
  → NSTextStorage attribute updates (background queue)
  → main: ruler + minimap invalidate dirty regions
  → existing 300ms debounce → PDF re-render (unchanged in Phase B)
```

### Error handling

- Missing asset path in `![…](…)` → render a "missing asset" placeholder attachment, log via `LuraDebugLog`, no crash.
- Highlighter regex failure on a line → fall back to plain attributes for that line, log, continue.
- `JetBrains Mono` not bundled → fall back to `SF Mono` via font descriptor cascade.

### Testing

- Snapshot tests for the editor with a sample doc covering all token types, light + dark.
- Manual: 10k-line doc scroll perf, asset embed/edit/remove, fold/unfold, live theme switch via System Settings.

### Estimate

3–5 days.

---

## Phase C — Performance

**Outcome:** App feels snappy. Real numbers, real fixes, real verification.

### Step 1 — Measurement (≈ 0.5 day)

Wire baseline numbers via Instruments and custom signposts. New file `quicklook/Shared/LuraSignpost.swift`.

| Metric            | Probe                                                         | Target          |
| ----------------- | ------------------------------------------------------------- | --------------- |
| Cold launch       | `os_signpost` from `main()` to first window paint             | < 400 ms        |
| Open file         | signpost from `openDocumentURL` to editor visible             | < 150 ms (<100 KB) |
| Typing latency    | signpost on `NSTextView.didChangeText` to next display frame  | < 16 ms         |
| Preview render    | signpost around `LuraRenderFFI.renderPDF`                     | p50 < 100 ms, p95 < 300 ms |
| Quick Look thumb  | wall time of `qlmanage -t -s 256 file.lura`                   | < 200 ms        |

Signpost output mirrored to `LuraDebugLog` and appended as CSV at `~/Library/Caches/.../perf.csv` for grep-friendly comparison.

### Step 2 — Likely fixes (rank by measured impact)

1. **Lazy Rust dylib load.** Today linked at launch. Switch to `dlopen` on first render call. Expected ~80–150 ms off cold launch.
2. **Incremental PDF render.** Current pipeline re-renders the entire document every debounce. Add block-level diff: tokenize → only re-render changed blocks → splice into prior PDF. Falls back to full render on structural change. Expected ~70% reduction on typical edit.
3. **Editor highlight off main thread.** Tokenize on a background `DispatchQueue`, apply attributes back on main in batches via `NSTextStorage.beginEditing()` / `endEditing()`. Keeps typing < 16 ms even on 10k-line docs.
4. **Recents thumbnail cache.** `LuraPreviewDiskCache` already caches full PDFs. Add a 256-px thumbnail variant generated once per content hash for the Welcome list.
5. **Bundle size.** Strip Rust dylib symbols (`strip -x`). `cargo build --release` profile additions: `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`. Smaller binary = faster mmap on launch.
6. **Quick Look extension warm path.** Extension currently rebuilds preview each call. Read sidecar `.lura.pdf` if present and the document hash matches → return instantly. Sidecar already written on Save (`LuraPreviewSidecar`).

### Step 3 — Verify

Re-run measurement script. Each fix is kept only if it moves a target metric ≥ 10% or hits target. Drop anything that does not pull its weight.

### Error handling

All fixes guarded behind a debug flag in `LuraDebugLog.perf` env var so any regression is one launch-flag away from being reverted at runtime.

### Testing

Before / after numbers committed to `docs/perf-baseline.md`. Manual smoke on the six user flows: cold launch, open small file, open large file, fast typing, save, Quick Look thumbnail in Finder.

### Estimate

2–3 days (measure 0.5d, fix 1.5d, verify and tune 1d).

---

## Acceptance criteria

Phase A:

- Double-click `.lura` from Finder (cold) opens directly in editor — no Welcome screen detour.
- Double-click `.lura` from Finder (warm) opens directly in editor.
- Recents click opens file with no permission prompt, including after reboot.
- Stale Recents entry is offered for removal, no crash.

Phase B:

- App matches Pages/Bear warm-paper aesthetic in both light and dark mode.
- Editor renders syntax-highlighted Lura source with line numbers, current-line highlight, fold ribbon, minimap, and inline asset thumbnails.
- App icon visible in Dock, Finder, and Cmd-Tab at all standard sizes.
- Theme switch via System Settings updates editor live without restart.

Phase C:

- All five baseline metrics measured and recorded.
- At least three of the six listed fixes shipped, each with ≥ 10% measured improvement on its target metric.
- `docs/perf-baseline.md` committed with before / after table.

## Open questions

None blocking.
