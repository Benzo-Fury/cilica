# WindowChrome

Window chrome is the 52px title band. Cilica draws it itself on Windows and Linux and keeps the real traffic lights on macOS.

- **macOS**: `titleBarStyle: Overlay` with traffic lights inset 20px from the left and centred in the band. The band is part of the sidebar's material.
- **Windows and Linux**: `decorations: false`; the band carries the sidebar toggle, the note title and Cilica's own minimise, maximise and close buttons (46px wide each, 10px glyphs). Close turns `danger` on hover. Use `data-tauri-drag-region` on the empty band; double-click maximises.
- **Windows 11 snap layouts**: hovering maximise must still show snap layouts. Call the window's native maximise button hit-test (HTMAXBUTTON) from the custom button.
- Window corners: `radius-window` on macOS and Linux; Windows 11 rounds its own corners (8px).
