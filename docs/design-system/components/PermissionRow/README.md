# PermissionRow

Permission rows explain and request one OS permission each, in Settings › Privacy. First-run setup uses its own permission buttons (SetupWindow).

Icon, title, one sentence on what Margin does with it (in plain words: "Hears the other people on Zoom, Meet or Teams. No bot joins the call."), and a status: a success chip when granted, a primary "Allow" button for the next missing one, a secondary "Open Settings" when the OS requires a trip to System Settings.

Rows update live: when the OS grants a permission, the button morphs into the success chip with `--spring-bouncy` and a soft success haptic on macOS.
