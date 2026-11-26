//! Turns on the webview's own spell checker. WebKit leaves it off for apps that embed it (macOS
//! WKWebView and Linux WebKitGTK alike), so `spellcheck="true"` alone draws no underlines. Windows'
//! WebView2 checks spelling by default, but its right-click menu has no "Add to dictionary"
//! (WebView2Feedback#3632), so the shell adds one (see [`windows_plugin`]).
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

/// Adds "Add to dictionary" under the spelling suggestions in every webview's right-click menu.
///
/// WebView2 checks spelling with the Windows spell checker (Chromium's default on Windows), and
/// Chromium's own Add to dictionary calls `ISpellChecker::Add` for each language. The item does the
/// same, so the word goes into the Windows user dictionary, is accepted by every app that uses it,
/// and can be removed in Windows Settings.
#[cfg(target_os = "windows")]
pub fn windows_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("spelling")
        .on_webview_ready(|webview| {
            let result = webview.with_webview(|platform| {
                if let Err(err) = windows_menu::hook(&platform.controller(), &platform.environment()) {
                    tracing::warn!("could not add Add to dictionary to the right-click menu: {err}");
                }
            });
            if let Err(err) = result {
                tracing::warn!("could not reach the webview for the spelling menu: {err}");
            }
        })
        .build()
}

#[cfg(target_os = "windows")]
mod windows_menu {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND, ICoreWebView2, ICoreWebView2_11, ICoreWebView2Controller,
        ICoreWebView2ContextMenuRequestedEventArgs, ICoreWebView2Environment, ICoreWebView2Environment9,
    };
    use webview2_com::{ContextMenuRequestedEventHandler, CustomItemSelectedEventHandler, take_pwstr};
    use windows::Win32::Globalization::{GetUserPreferredUILanguages, ISpellCheckerFactory, MUI_LANGUAGE_NAME, SpellCheckerFactory};
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IStream};
    use windows::core::{BOOL, HSTRING, Interface, PWSTR, Result};

    const LABEL: &str = "Add to dictionary";
    /// Name WebView2 gives the spelling suggestion items in its default menu.
    const SUGGESTION: &str = "spellcheck";
    /// Turning `spellcheck` off and on clears every underline and has Blink check the text again,
    /// so the word just added loses its underline. Left alone when Check spelling is off.
    const RECHECK: &str =
        "(() => { const root = document.documentElement; if (root.spellcheck) { root.spellcheck = false; root.spellcheck = true; } })();";

    /// Listens for the webview's right-click menu. Runs once per webview, on the main thread.
    pub fn hook(controller: &ICoreWebView2Controller, environment: &ICoreWebView2Environment) -> Result<()> {
        unsafe {
            let webview: ICoreWebView2_11 = controller.CoreWebView2()?.cast()?;
            let environment: ICoreWebView2Environment9 = environment.cast()?;
            let handler = ContextMenuRequestedEventHandler::create(Box::new(move |sender, args| {
                if let (Some(sender), Some(args)) = (sender, args)
                    && let Err(err) = add_item(&sender, &environment, &args)
                {
                    tracing::warn!("could not add Add to dictionary to this menu: {err}");
                }
                Ok(())
            }));
            let mut token = 0;
            webview.add_ContextMenuRequested(&handler, &mut token)
        }
    }

    /// Puts the item after the last suggestion. Blink selects a misspelt word when it is
    /// right-clicked and only offers suggestions when the selection is exactly that word, so the
    /// selected text is the word to add.
    unsafe fn add_item(
        webview: &ICoreWebView2,
        environment: &ICoreWebView2Environment9,
        args: &ICoreWebView2ContextMenuRequestedEventArgs,
    ) -> Result<()> {
        unsafe {
            let target = args.ContextMenuTarget()?;
            let mut editable = BOOL::default();
            target.IsEditable(&mut editable)?;
            if !editable.as_bool() {
                return Ok(());
            }

            // Find the spelling suggestions; without them the click was not on a misspelt word.
            let items = args.MenuItems()?;
            let mut count = 0;
            items.Count(&mut count)?;
            let mut last_suggestion = None;
            for index in 0..count {
                let mut name = PWSTR::null();
                items.GetValueAtIndex(index)?.Name(&mut name)?;
                if take_pwstr(name).eq_ignore_ascii_case(SUGGESTION) {
                    last_suggestion = Some(index);
                }
            }
            let Some(last_suggestion) = last_suggestion else { return Ok(()) };

            let mut selection = PWSTR::null();
            target.SelectionText(&mut selection)?;
            let Some(word) = super::dictionary_word(&take_pwstr(selection)) else { return Ok(()) };

            // The item, and what choosing it does.
            let item = environment.CreateContextMenuItem(
                &HSTRING::from(LABEL),
                None::<&IStream>,
                COREWEBVIEW2_CONTEXT_MENU_ITEM_KIND_COMMAND,
            )?;
            let webview = webview.clone();
            let selected = CustomItemSelectedEventHandler::create(Box::new(move |_, _| {
                match add_word(&word) {
                    Ok(()) => {
                        if let Err(err) = webview.ExecuteScript(&HSTRING::from(RECHECK), None) {
                            tracing::warn!("could not recheck spelling: {err}");
                        }
                    }
                    Err(err) => tracing::warn!("could not add a word to the Windows dictionary: {err}"),
                }
                Ok(())
            }));
            let mut token = 0;
            item.add_CustomItemSelected(&selected, &mut token)?;
            items.InsertValueAtIndex(last_suggestion + 1, &item)
        }
    }

    /// Adds `word` to the Windows user dictionary of each display language Windows can spell-check,
    /// which is what WebView2 checks with when the app does not pick a language.
    fn add_word(word: &str) -> Result<()> {
        unsafe {
            let factory: ISpellCheckerFactory = CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER)?;
            let word = HSTRING::from(word);
            for language in preferred_languages()? {
                let language = HSTRING::from(language);
                if factory.IsSupported(&language)?.as_bool() {
                    factory.CreateSpellChecker(&language)?.Add(&word)?;
                }
            }
            Ok(())
        }
    }

    /// The user's display languages as tags such as `en-GB`, most preferred first.
    fn preferred_languages() -> Result<Vec<String>> {
        unsafe {
            let (mut languages, mut length) = (0, 0);
            GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut languages, None, &mut length)?;
            let mut buffer = vec![0u16; length as usize];
            GetUserPreferredUILanguages(MUI_LANGUAGE_NAME, &mut languages, Some(PWSTR(buffer.as_mut_ptr())), &mut length)?;
            Ok(super::split_multi_string(&buffer))
        }
    }
}

/// The selected text as a word for the dictionary: trimmed, one word, at most 64 characters.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn dictionary_word(selection: &str) -> Option<String> {
    let word = selection.trim();
    let fits = !word.is_empty() && word.chars().count() <= 64 && !word.chars().any(char::is_whitespace);
    fits.then(|| word.to_owned())
}

/// A Windows multi-string (`a\0b\0\0`) as its strings.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn split_multi_string(buffer: &[u16]) -> Vec<String> {
    buffer.split(|&unit| unit == 0).filter(|part| !part.is_empty()).map(String::from_utf16_lossy).collect()
}

/// `en_GB.UTF-8` → `en_GB`. Falls back to `en_US` for an unset or `C`/`POSIX` locale.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn language_from_env(lang: Option<&str>) -> String {
    let code = lang.unwrap_or_default().split(['.', '@']).next().unwrap_or_default();
    if code.is_empty() || code == "C" || code == "POSIX" { "en_US".into() } else { code.into() }
}

#[cfg(test)]
mod tests {
    use super::{dictionary_word, language_from_env, split_multi_string};

    #[test]
    fn selection_becomes_one_word() {
        assert_eq!(dictionary_word("  Cilica \n").as_deref(), Some("Cilica"));
        assert_eq!(dictionary_word("two words"), None);
        assert_eq!(dictionary_word("   "), None);
        assert_eq!(dictionary_word(&"a".repeat(65)), None);
    }

    #[test]
    fn multi_string_splits_on_nulls() {
        let buffer: Vec<u16> = "en-GB\0sl-SI\0\0".encode_utf16().collect();
        assert_eq!(split_multi_string(&buffer), ["en-GB", "sl-SI"]);
        assert!(split_multi_string(&[0, 0]).is_empty());
    }

    #[test]
    fn locale_becomes_a_dictionary_name() {
        assert_eq!(language_from_env(Some("en_GB.UTF-8")), "en_GB");
        assert_eq!(language_from_env(Some("de_DE@euro")), "de_DE");
        assert_eq!(language_from_env(Some("C")), "en_US");
        assert_eq!(language_from_env(None), "en_US");
    }
}
