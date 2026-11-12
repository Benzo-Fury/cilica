//! settings (one JSON document under the key "settings"). Owner: Core agent.
//!
//! The settings document is read tolerantly: missing fields take their defaults, unknown fields
//! are ignored on read and kept on write (so an older build does not erase a newer build's
//! settings), and a field whose stored value no longer fits its type falls back to its default
//! without losing the rest of the document.
//!
//! [`Db::get_value`] / [`Db::set_value`] are small key → JSON helpers for caches (prompt chips,
//! model catalogue, window hints). Keys are free-form; prefix them with your area (`assistant.`).

use margin_domain::Settings;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::{Db, StoreResult};

/// The key of the settings document.
pub const SETTINGS_KEY: &str = "settings";

impl Db {
    /// The settings, with defaults for anything missing or unreadable.
    pub async fn get_settings(&self) -> StoreResult<Settings> {
        let mut settings = match self.get_value(SETTINGS_KEY).await? {
            Some(stored) => settings_from_value(&stored),
            None => Settings::default(),
        };
        // Models left over from another provider would make every request fail.
        settings.ai.fit_models_to_provider();
        Ok(settings)
    }

    /// Replace the settings. Unknown fields already stored are kept.
    pub async fn set_settings(&self, settings: &Settings) -> StoreResult<()> {
        let new = serde_json::to_value(settings)?;
        let template = serde_json::to_value(Settings::default())?;
        let merged = match self.get_value(SETTINGS_KEY).await? {
            Some(stored) => keep_unknown_fields(stored, new, &template),
            None => new,
        };
        self.set_value(SETTINGS_KEY, &merged).await
    }

    /// A stored JSON value, or `None` when the key is not set. A value that is not valid JSON
    /// (written by hand, say) reads as `None`.
    pub async fn get_value(&self, key: &str) -> StoreResult<Option<Value>> {
        let text: Option<String> = sqlx::query_scalar("SELECT value_json FROM settings WHERE key = ?").bind(key).fetch_optional(self.pool()).await?;
        Ok(text.and_then(|text| match serde_json::from_str(&text) {
            Ok(value) => Some(value),
            Err(error) => {
                tracing::warn!(key, %error, "ignoring a stored value that is not valid JSON");
                None
            }
        }))
    }

    /// Store a JSON value under `key`, replacing what was there.
    pub async fn set_value(&self, key: &str, value: &Value) -> StoreResult<()> {
        sqlx::query("INSERT INTO settings (key, value_json) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json")
            .bind(key)
            .bind(serde_json::to_string(value)?)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Remove a key. Removing a key that is not set is fine.
    pub async fn delete_value(&self, key: &str) -> StoreResult<()> {
        sqlx::query("DELETE FROM settings WHERE key = ?").bind(key).execute(self.pool()).await?;
        Ok(())
    }

    /// [`Db::get_value`] decoded into `T`; `None` when missing or when it no longer fits `T`.
    pub async fn get_json<T: DeserializeOwned>(&self, key: &str) -> StoreResult<Option<T>> {
        Ok(self.get_value(key).await?.and_then(|value| serde_json::from_value(value).ok()))
    }

    /// [`Db::set_value`] from any serialisable value.
    pub async fn set_json<T: Serialize>(&self, key: &str, value: &T) -> StoreResult<()> {
        self.set_value(key, &serde_json::to_value(value)?).await
    }
}

/// Read settings from stored JSON, keeping every field that still fits and defaulting the rest.
pub fn settings_from_value(stored: &Value) -> Settings {
    if let Ok(settings) = serde_json::from_value::<Settings>(stored.clone()) {
        return settings;
    }
    // Something no longer fits (a removed enum variant, a changed type). Copy the stored leaves
    // onto the defaults one at a time and keep each one only if the whole document still reads.
    let mut accepted = serde_json::to_value(Settings::default()).unwrap_or(Value::Null);
    let mut path = Vec::new();
    overlay(&mut accepted, stored, &mut path);
    serde_json::from_value(accepted).unwrap_or_default()
}

fn overlay(accepted: &mut Value, stored: &Value, path: &mut Vec<String>) {
    let Value::Object(fields) = stored else {
        return;
    };
    for (key, value) in fields {
        path.push(key.clone());
        let target_is_object = pointer(accepted, path).is_some_and(Value::is_object);
        if target_is_object && value.is_object() {
            overlay(accepted, value, path);
        } else {
            let mut candidate = accepted.clone();
            set_at(&mut candidate, path, value.clone());
            if serde_json::from_value::<Settings>(candidate.clone()).is_ok() {
                *accepted = candidate;
            }
        }
        path.pop();
    }
}

fn pointer<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    path.iter().try_fold(value, |v, key| v.get(key))
}

fn set_at(root: &mut Value, path: &[String], value: Value) {
    let Some((last, parents)) = path.split_last() else {
        return;
    };
    let mut node = root;
    for key in parents {
        let Value::Object(map) = node else { return };
        node = map.entry(key.clone()).or_insert_with(|| Value::Object(Map::new()));
    }
    if let Value::Object(map) = node {
        map.insert(last.clone(), value);
    }
}

/// `new` plus any fields of `stored` that this build does not know. Recurses only into structs
/// (objects that have fields in the default document); maps such as `keybinds` are replaced
/// whole so removing an entry sticks.
fn keep_unknown_fields(stored: Value, new: Value, template: &Value) -> Value {
    match (stored, new) {
        (Value::Object(stored), Value::Object(mut new)) if template.as_object().is_some_and(|t| !t.is_empty()) => {
            for (key, old) in stored {
                match new.remove(&key) {
                    Some(value) => {
                        let sub_template = template.get(&key).cloned().unwrap_or(Value::Null);
                        new.insert(key, keep_unknown_fields(old, value, &sub_template));
                    }
                    None if template.get(&key).is_none() => {
                        new.insert(key, old);
                    }
                    None => {}
                }
            }
            Value::Object(new)
        }
        (_, new) => new,
    }
}

#[cfg(test)]
mod tests {
    use margin_domain::{AccentPreset, ThemeMode};
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn defaults_when_missing() {
        let db = Db::open_in_memory().await.unwrap();
        assert_eq!(db.get_settings().await.unwrap(), Settings::default());
    }

    #[tokio::test]
    async fn round_trip() {
        let db = Db::open_in_memory().await.unwrap();
        let mut settings = Settings::default();
        settings.appearance.theme = ThemeMode::Dark;
        settings.appearance.transparency = false;
        settings.recording.auto_detect = false;
        settings.keybinds.insert("recording.toggle".into(), "Mod+Alt+R".into());
        settings.launch_at_login = true;
        db.set_settings(&settings).await.unwrap();
        assert_eq!(db.get_settings().await.unwrap(), settings);

        // Overwrite and make sure removing a keybind override sticks.
        settings.keybinds.clear();
        db.set_settings(&settings).await.unwrap();
        assert_eq!(db.get_settings().await.unwrap(), settings);
    }

    #[tokio::test]
    async fn tolerates_missing_and_unknown_fields() {
        let db = Db::open_in_memory().await.unwrap();
        db.set_value(
            SETTINGS_KEY,
            &json!({
                "appearance": { "theme": "light", "future_flag": 3 },
                "brand_new_section": { "x": 1 },
                "launch_at_login": true
            }),
        )
        .await
        .unwrap();
        let settings = db.get_settings().await.unwrap();
        assert_eq!(settings.appearance.theme, ThemeMode::Light);
        assert!(settings.appearance.transparency, "missing fields take defaults");
        assert!(settings.launch_at_login);
        assert_eq!(settings.recording, Settings::default().recording);

        // Writing back keeps what this build does not understand.
        db.set_settings(&settings).await.unwrap();
        let stored = db.get_value(SETTINGS_KEY).await.unwrap().unwrap();
        assert_eq!(stored["brand_new_section"], json!({ "x": 1 }));
        assert_eq!(stored["appearance"]["future_flag"], json!(3));
        assert_eq!(stored["appearance"]["theme"], json!("light"));
    }

    #[tokio::test]
    async fn a_bad_field_falls_back_alone() {
        let db = Db::open_in_memory().await.unwrap();
        db.set_value(
            SETTINGS_KEY,
            &json!({
                "appearance": { "theme": "dark", "accent": "ultraviolet", "transparency": "yes" },
                "recording": { "auto_detect": false, "audio_retention_days": -4 },
                "keybinds": { "recording.toggle": "Mod+Shift+K", "broken": 7 },
                "onboarding_done": true
            }),
        )
        .await
        .unwrap();
        let settings = db.get_settings().await.unwrap();
        assert_eq!(settings.appearance.theme, ThemeMode::Dark);
        assert_eq!(settings.appearance.accent, AccentPreset::Amber);
        assert!(settings.appearance.transparency);
        assert!(!settings.recording.auto_detect);
        assert_eq!(settings.recording.audio_retention_days, 30);
        assert_eq!(settings.keybinds.get("recording.toggle").map(String::as_str), Some("Mod+Shift+K"));
        assert!(!settings.keybinds.contains_key("broken"));
        assert!(settings.onboarding_done);
    }

    #[tokio::test]
    async fn not_an_object_reads_as_defaults() {
        let db = Db::open_in_memory().await.unwrap();
        db.set_value(SETTINGS_KEY, &json!([1, 2, 3])).await.unwrap();
        assert_eq!(db.get_settings().await.unwrap(), Settings::default());
        sqlx::query("UPDATE settings SET value_json = '{not json' WHERE key = 'settings'").execute(db.pool()).await.unwrap();
        assert_eq!(db.get_settings().await.unwrap(), Settings::default());
    }

    #[tokio::test]
    async fn generic_values() {
        let db = Db::open_in_memory().await.unwrap();
        assert_eq!(db.get_value("assistant.chips").await.unwrap(), None);
        db.set_value("assistant.chips", &json!(["a", "b"])).await.unwrap();
        assert_eq!(db.get_value("assistant.chips").await.unwrap(), Some(json!(["a", "b"])));
        db.set_json("stt.catalogue", &vec![1u32, 2, 3]).await.unwrap();
        assert_eq!(db.get_json::<Vec<u32>>("stt.catalogue").await.unwrap(), Some(vec![1, 2, 3]));
        assert_eq!(db.get_json::<String>("stt.catalogue").await.unwrap(), None);
        db.delete_value("assistant.chips").await.unwrap();
        assert_eq!(db.get_value("assistant.chips").await.unwrap(), None);
    }
}
