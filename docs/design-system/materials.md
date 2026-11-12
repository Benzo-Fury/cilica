# Materials

Margin uses two materials. Content never sits on glass, and glass never sits on glass.

## 1. The sheet (opaque shell)

The window is one opaque sheet with no card and no see-through panels, so it looks the same on every wallpaper and every OS.

| Region | Ground |
| --- | --- |
| Sidebar and its title band | `bg-sunken` |
| Note, transcript and pages | `bg-canvas` |
| Assistant panel | `bg-canvas` |

The three regions sit flush and are split by 0.5px `line-strong` hairlines. Nothing in the shell has a radius or a shadow. Cards inside the note (diff cards, settings groups) use `bg-raised`.

No native window material is applied (`WINDOW_MATERIAL` in `src-tauri/src/shell/material.rs` is off). The code for Liquid Glass, vibrancy, Mica and Acrylic stays there in case a see-through shell returns.

## 2. In-app glass (floating controls)

Controls that float over the canvas are frosted glass made in CSS:

- `nt-glass`: `glass-fill`, `backdrop-filter: blur(blur-glass) saturate(180%)`, a 0.5px `glass-edge`, `shadow-float`. No inner highlight. For the recorder capsule, floating toolbars, toasts, the "Jump to live" chip.
- `nt-glass-strong`: `glass-fill-strong` with `blur-glass-strong`. For anything with text to read: menus, the command palette, popovers.
- Glass buttons and large glass surfaces carry a soft specular highlight that follows the pointer.
- The Dynamic Island is its own window: true black on Macs with a notch, dark glass elsewhere.

## Rules

- No inner light on glass or any other surface: no `inset 0 1px 0` top rim, no inner glow. The edge comes from the 0.5px `glass-edge` and the shadow alone.
- Dialogs are opaque `bg-raised`: a decision deserves a solid surface.
- Don't stack glass on glass (a menu opened from the recorder capsule renders below it in `glass-fill-strong` with a stronger blur, never over it).
- Keep glass small: it is for controls, not regions. The largest in-app glass surface is the command palette.
- Performance: backdrop blur is expensive in WebKit. Keep at most three blurred layers on screen, never animate the blur radius, and drop to opaque fills while a window is being resized.
