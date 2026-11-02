use std::collections::BTreeSet;

use ela_core::Store;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::CommandResult;
use crate::saved::{Saved, with_store};

// Named as `KeyboardEvent.code` names them, which is what the webview compares against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub enum HostKeyCode {
    ControlRight,
    AltRight,
    ShiftRight,
    ContextMenu,
    ScrollLock,
    Pause,
}

impl HostKeyCode {
    fn parse(text: &str) -> Option<Self> {
        Self::deserialize(IntoDeserializer::<serde::de::value::Error>::into_deserializer(text)).ok()
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::ControlRight => "ControlRight",
            Self::AltRight => "AltRight",
            Self::ShiftRight => "ShiftRight",
            Self::ContextMenu => "ContextMenu",
            Self::ScrollLock => "ScrollLock",
            Self::Pause => "Pause",
        }
    }
}

/// `None` when nothing is chosen yet, and also when the file names a key this version doesn't
/// offer, which stays in the file until another key is chosen.
fn saved_host_key(store: &Store) -> CommandResult<Option<HostKeyCode>> {
    let settings = store.app_settings()?;
    Ok(settings.host_key.as_deref().and_then(HostKeyCode::parse))
}

fn save_host_key(store: &Store, code: HostKeyCode) -> CommandResult<()> {
    Ok(store.update_app_settings(|settings| settings.host_key = Some(code.as_str().to_owned()))?)
}

#[tauri::command]
#[specta::specta]
pub async fn host_key(saved: State<'_, Saved>) -> CommandResult<Option<HostKeyCode>> {
    with_store(&saved, |store| saved_host_key(store))
}

#[tauri::command]
#[specta::specta]
pub async fn set_host_key(saved: State<'_, Saved>, code: HostKeyCode) -> CommandResult<()> {
    with_store(&saved, |store| save_host_key(store, code))
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct TerminalFont {
    pub family: Option<String>,
    pub size: Option<u16>,
}

fn saved_terminal_font(store: &Store) -> CommandResult<TerminalFont> {
    let settings = store.app_settings()?;
    Ok(TerminalFont {
        family: settings.terminal_font,
        size: settings.terminal_font_size,
    })
}

fn save_terminal_font(store: &Store, font: TerminalFont) -> CommandResult<()> {
    Ok(store.update_app_settings(|settings| {
        settings.terminal_font = font.family;
        settings.terminal_font_size = font.size;
    })?)
}

#[tauri::command]
#[specta::specta]
pub async fn terminal_font(saved: State<'_, Saved>) -> CommandResult<TerminalFont> {
    with_store(&saved, |store| saved_terminal_font(store))
}

#[tauri::command]
#[specta::specta]
pub async fn set_terminal_font(saved: State<'_, Saved>, font: TerminalFont) -> CommandResult<()> {
    with_store(&saved, |store| save_terminal_font(store, font))
}

fn system_monospace_fonts() -> Vec<String> {
    let mut fonts = fontdb::Database::new();
    fonts.load_system_fonts();
    let families: BTreeSet<String> = fonts
        .faces()
        .filter(|face| face.monospaced)
        .filter_map(|face| face.families.first().map(|(name, _)| name.clone()))
        .collect();
    families.into_iter().collect()
}

// Reading every font file on the system takes long enough to be noticed on a slow disk.
#[tauri::command]
#[specta::specta]
pub async fn monospace_fonts() -> CommandResult<Vec<String>> {
    Ok(tauri::async_runtime::spawn_blocking(system_monospace_fonts).await?)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn the_host_key_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(saved_host_key(&store).unwrap(), None);

        save_host_key(&store, HostKeyCode::ScrollLock).unwrap();

        let store = Store::open(dir.path()).unwrap();
        assert_eq!(
            saved_host_key(&store).unwrap(),
            Some(HostKeyCode::ScrollLock)
        );
        let text = fs::read_to_string(dir.path().join("settings.toml")).unwrap();
        assert!(text.contains("host_key = \"ScrollLock\""), "{text}");
    }

    #[test]
    fn the_terminal_font_survives_a_restart_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(
            saved_terminal_font(&store).unwrap(),
            TerminalFont::default()
        );

        let font = TerminalFont {
            family: Some("JetBrains Mono".to_owned()),
            size: Some(15),
        };
        save_terminal_font(&store, font.clone()).unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(saved_terminal_font(&store).unwrap(), font);

        save_terminal_font(&store, TerminalFont::default()).unwrap();
        let text = fs::read_to_string(dir.path().join("settings.toml")).unwrap();
        assert!(!text.contains("terminal_font"), "{text}");
    }

    #[test]
    fn every_host_key_is_written_as_it_is_read() {
        for code in [
            HostKeyCode::ControlRight,
            HostKeyCode::AltRight,
            HostKeyCode::ShiftRight,
            HostKeyCode::ContextMenu,
            HostKeyCode::ScrollLock,
            HostKeyCode::Pause,
        ] {
            assert_eq!(HostKeyCode::parse(code.as_str()), Some(code));
            assert_eq!(
                serde_json::to_value(code).unwrap(),
                serde_json::json!(code.as_str())
            );
        }
    }

    #[test]
    fn a_key_this_version_does_not_offer_reads_as_none_and_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let text = "schema_version = 1\nhost_key = \"MetaRight\"\n";
        fs::write(&path, text).unwrap();
        let store = Store::open(dir.path()).unwrap();

        assert_eq!(saved_host_key(&store).unwrap(), None);
        store
            .update_app_settings(|settings| settings.vault_idle_minutes = Some(5))
            .unwrap();
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("host_key = \"MetaRight\"")
        );
    }
}
