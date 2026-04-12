# First Quick Look open: strategy and measurements

## Chosen directions (implemented)

1. **Sidecar PDF** — `<basename>.preview.pdf` next to the source. Quick Look uses it when its modification time is not older than the source (no Rust). Written by:
   - `lura convert --format pdf --preview-sidecar …`
   - `lura render … --preview-sidecar`
   - Lura app on **Save** (if the preview pane has a successful PDF).

2. **Shared disk cache (App Group)** — `group.com.fallowlone.lura`: host and QL extension share `Library/Caches/LuraPreviewRender/` keyed by SHA256 of UTF-8 bytes. The editor **prewarms** this cache on every successful debounced preview, so Finder Quick Look can be cold-fast after the file was opened in Lura once.

Not implemented in this pass (still valid follow-ups): Quick Look **thumbnail** extension, **two-phase** preview (first page then full PDF), **lura watch** background prerender.

## Quick Look resolution order

1. Fresh sidecar (`*.preview.pdf`, mtime ≥ source).
2. Shared SHA cache (App Group, or per-target Caches if group unavailable).
3. Full `lura_render_pdf` pipeline.

## Profiling note (`showcase-large.fol`)

Timed on one dev machine (release binary, no `cargo` wrapper):

```sh
/usr/bin/time -p ./target/release/lura render examples/showcase-large.fol --output /tmp/out.pdf
```

Observed **real ~0.17s** after warm filesystem cache. For deeper hotspots (layout vs pagination vs PDF backend), use Instruments or `cargo flamegraph` on the same command.

## App Group registration

For distribution through the Mac App Store or notarized builds, register `group.com.fallowlone.lura` in the Apple Developer portal and keep the same ID in entitlements. Ad hoc local signing with `install-preview.sh` still enables the group container when the entitlement is present.
