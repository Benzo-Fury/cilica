//! Turns on the webview's own spell checker. WebKit leaves it off for apps that embed it (macOS
//! WKWebView and Linux WebKitGTK alike), so `spellcheck="true"` alone draws no underlines. Windows'
//! WebView2 checks spelling by default.
//!
//! Once it is on here, Settings › General › Writing › Check spelling decides what is underlined:
//! the UI sets the `spellcheck` attribute on `<html>` and every text box inherits it.

/// WebKit reads this once per launch, so it must be set before the first webview exists.
/// Cilica's own setting is the switch people use, so this is set on every launch rather than
/// registered as a default the right-click menu could leave off.
#[cfg(target_os = "macos")]
pub fn enable_before_webviews() {
    use objc2_foundation::{NSString, NSUserDefaults};
    let defaults = NSUserDefaults::standardUserDefaults();
    defaults.setBool_forKey(true, &NSString::from_str("WebContinuousSpellCheckingEnabled"));
}

/// The checker belongs to the webview's context, which only exists once a webview does: called for
/// every page load, and harmless to repeat.
#[cfg(target_os = "linux")]
pub fn enable(webview: &tauri::Webview) {
    let language = language_from_env(std::env::var("LANG").ok().as_deref());
    let result = webview.with_webview(move |platform| {
        use webkit2gtk::{WebContextExt, WebViewExt};
        if let Some(context) = platform.inner().context() {
            context.set_spell_checking_enabled(true);
            context.set_spell_checking_languages(&[language.as_str()]);
        }
    });
    if let Err(err) = result {
        tracing::warn!("could not turn on spell checking: {err}");
    }
}

/// `en_GB.UTF-8` → `en_GB`. Falls back to `en_US` for an unset or `C`/`POSIX` locale.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn language_from_env(lang: Option<&str>) -> String {
    let code = lang.unwrap_or_default().split(['.', '@']).next().unwrap_or_default();
    if code.is_empty() || code == "C" || code == "POSIX" { "en_US".into() } else { code.into() }
}

#[cfg(test)]
mod tests {
    use super::language_from_env;

    #[test]
    fn locale_becomes_a_dictionary_name() {
        assert_eq!(language_from_env(Some("en_GB.UTF-8")), "en_GB");
        assert_eq!(language_from_env(Some("de_DE@euro")), "de_DE");
        assert_eq!(language_from_env(Some("C")), "en_US");
        assert_eq!(language_from_env(None), "en_US");
    }
}
