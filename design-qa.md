# Go Durandal visual fidelity

final result: passed

## Source and capture

- Canonical product: [Blumenwagen/durandal](https://github.com/Blumenwagen/durandal/tree/6f8e9621ca3aab548081cfac324df278565affbb), commit `6f8e9621ca3aab548081cfac324df278565affbb`.
- Art direction: the Go repository's `img/showcase.png`, `internal/styles/styles.go`, and native component/layout renderers.
- Source visual truth: [Go renderer capture](docs/go-reference.png). This uses the actual Go components, not a reimplementation of their appearance.
- Implementation: [Rust renderer capture](docs/dashboard.png).
- Full comparison: [Go and Rust side by side](docs/go-parity.png).
- Focused comparison: [masthead, Sentinel, CPU and table detail](docs/go-parity-detail.png).
- Viewport: 120 columns × 36 rows, non-root default theme, process focus. Both have the same host, CPU, memory, disks, processes, containers, and deterministic demo history. The Go history and Rust history use their respective native chart implementations.
- Both captures: 1356 × 900 pixels, including identical 18-pixel outer padding. Normalization uses 11 × 24 pixel terminal cells, Menlo 18 regular and bold, the same black terminal background, and no scaling between captures. CSS/device pixel ratio does not apply to this native terminal UI.
- Reference capture decodes Go's ANSI styles; Rust capture uses actual Ratatui cells, colors and font modifiers. OS window chrome, terminal transparency and desktop wallpaper from the original showcase are excluded from both.

## Comparison history

1. Initial comparison: blocked. The previous Rust UI used pastel accents, closed boxes, a global health banner, a sparse resource sidebar and muted selection. These were P1 differences in palette, hierarchy and surface treatment relative to the Go reference.
2. First parity capture: blocked. Canonical palette, reversed masthead/section strips, accent rails, grey table bands, solid lime selection, left-column Sentinel/resources and keyboard badges were restored. The matched comparison still showed a P2 difference in CPU chart/core-row proportions and missing truncation indicators in command cells.
3. Final capture: passed. CPU graph/core-row allocation now follows Go's rhythm; command truncation includes a grapheme-aware ellipsis. Full-view and focused comparisons show the same visual hierarchy and neon identity. No actionable P0/P1/P2 differences remain for the intended non-root dashboard state.

## Required fidelity surfaces

- **Typography:** same terminal-controlled monospace family, regular/bold weight, cell pitch and rasterization in both captures. Reversed headings use tracked uppercase labels. Numeric table values align. The terminal, rather than either application, selects the user's actual font.
- **Spacing/layout:** matched 35%/65% columns; Sentinel/CPU/memory/network/storage stack at left, processes and Docker at right. Header, panel strips and one-line badge footer follow Go. A small extra network row keeps the upload graph visible; tables use available rows rather than preserving Go's unused bottom padding.
- **Colors/tokens:** exact Go RGB values for lime `#BFFF00`, pink `#FF007F`, cyan `#00F0FF`, black `#0A0A0A`, navy `#12121F`, muted grey `#8A8A9E`, rule grey `#4A4A5A`, text `#D8D8E8`, amber `#FFB800` and red `#FF2244`. Dim mode maps accents to Go's dim variants while leaving neutral text readable.
- **Image/asset fidelity:** the product is composed of native terminal glyphs and widgets in both languages. There are no raster assets, generated illustrations, substitute logos or decorative approximations in the running dashboard. Committed PNGs are demo documentation captures only.
- **Copy/content:** Durandal branding, systems-monitor masthead, section names, CPU sort indicator, short byte units, container status and keyboard badge vocabulary follow Go. Rust adds active focus, contextual help and pause/search controls where its interaction model differs.

## Intentional differences

- Compact terminals retain the Rust adaptive summary and focused table; Go's wide layout is the aesthetic reference, not a requirement to recreate its overflow at small sizes.
- Native Ratatui sparklines quantize bar heights slightly differently from Go's glyph renderer. Network plots use a shared maximum over the visible measured window. These are acceptable chart-rendering differences, not fabricated history.
- Rust includes all fitting container rows, explicit running/stopped counts, captured-target confirmations, scrollable dialogs, and a dedicated help key. These retain the earlier UX work.
- GPU discovery remains honest about unavailable sensors; no temperature or utilization values are invented to imitate the original showcase.
- Root-specific alternate colors and desktop transparency are outside this non-root visual comparison.

## Verification

- Native buffer tests protect the canonical palette, reversed panel headers, solid selection, dim accents, and history behavior.
- History advances only on collected samples, is bounded, and remains frozen during pause/redraws. CPU mini-bars use Linux per-core counter deltas.
- Existing navigation, confirmation, search, Unicode and small-viewport regression checks remain in place.
- Inspected the default matched comparison, focused detail, and compact render. Wide and dimmed demo states are reproducible with `cargo run --example dashboard -- --width 180 --height 44` and `--mode dimmed`.
- A terminal smoke check covers help, search, focus, pause/resume, resize, quit and terminal restoration. Linux CI checks actual `/proc` collection, including the new per-core values.

## Implementation checklist

- [x] Use the canonical Go palette and magazine panels.
- [x] Restore header, table, selection and keyboard badge styling.
- [x] Restore sampled history and measured core bars.
- [x] Preserve navigation and compact terminal support.
- [x] Compare same-state captures at the same terminal size and density.
