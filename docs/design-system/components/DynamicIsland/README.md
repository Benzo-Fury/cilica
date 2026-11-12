# DynamicIsland

The Dynamic Island is a small always-on-top window that shows recording state while Margin is in the background (for example, while you are in Zoom).

**Placement**: top-centre of the active screen. On Macs with a notch it is black (#000) and sits flush under the menu bar, so it reads as part of the notch. On Windows, Linux and Macs without a notch it is dark glass (`data-glass`), 8px below the top edge.

**Compact** (168 × 32): breathing dot, five-bar meter, timer.
**Expanded** (360 × 76, on hover or click): meeting title, status line, Pause and Stop. Width, height and corner radius morph together with `--spring-bouncy`; compact content blurs out as full content scales in.

**Implementation**: a second Tauri window, `transparent: true`, `decorations: false`, `alwaysOnTop: true`, `skipTaskbar: true`, `focus: false`, visible on all workspaces, click-through outside the pill. It shows only while recording and hides when Margin's main window is focused.
