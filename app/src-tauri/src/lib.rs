mod benchmark_log;
mod error;
mod files;
mod known_hosts;
mod recent;
mod saved;
mod screenshot;
mod session;
mod settings;
mod trash;
mod vault;

use std::sync::Arc;

use ela_session::{ProtocolRegistry, SessionManager};
use ela_workspace::Workspace;
use tauri::Manager;

type Manager_ = Arc<SessionManager>;

fn commands() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(tauri_specta::collect_commands![
            session::list_protocols,
            session::parse_quick_connect,
            session::open_session,
            session::open_saved_session,
            session::session_input,
            session::session_answer,
            session::session_ack,
            session::close_session,
            screenshot::save_screenshot,
            benchmark_log::record_benchmark,
            saved::list_tree,
            saved::save_connection,
            saved::save_folder,
            saved::import_ssh_config,
            saved::trash_item,
            saved::restore_items,
            trash::list_trash,
            trash::empty_trash,
            saved::move_item,
            saved::move_changes,
            saved::resolve_connection,
            saved::outside_settings,
            files::files_list,
            files::files_download,
            files::files_download_folder,
            files::files_upload,
            files::files_upload_folder,
            files::files_upload_dropped,
            files::files_rename,
            files::files_make_folder,
            files::files_delete,
            files::files_cancel,
            known_hosts::known_hosts_list,
            known_hosts::known_hosts_remove,
            vault::vault_info,
            vault::vault_create,
            vault::vault_unlock,
            vault::vault_change_master,
            vault::vault_set_idle_minutes,
            vault::vault_lock,
            vault::vault_store_secret,
            vault::vault_remove_secret,
            vault::vault_credential_state,
            vault::vault_key_passphrases,
            vault::vault_forget_key_passphrase,
            settings::host_key,
            settings::set_host_key,
            settings::terminal_font,
            settings::set_terminal_font,
            settings::monospace_fonts,
            recent::list_recent,
            recent::forget_recent,
            recent::clear_recent,
        ])
        .typ::<files::FileDrag>()
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        // The counters are far below 2^53, and JSON carries them as plain numbers.
        .dangerously_cast_bigints_to_number()
}

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("ELA_LOG")
                .unwrap_or_else(|_| "info,ironrdp=warn,russh=warn".into()),
        )
        .init();

    let saved = saved::open_default();
    let vault = vault::open_default(&saved);
    vault::lock_when_idle(&vault);
    let history = recent::History::open_default();

    let ssh = ela_ssh::SshProtocol::default()
        .remembering_passphrases(Arc::new(vault::VaultPassphrases(vault.clone())));
    let jump_hosts = ssh.transport();
    let mut registry = ProtocolRegistry::new();
    registry
        .register(ssh)
        .register(ela_rdp::RdpProtocol::default().through_jump_hosts(jump_hosts))
        .register(ela_bench::BenchTerminal)
        .register(ela_bench::BenchFramebuffer);
    let mut workspace = Workspace::new(
        saved.clone().map_err(|e| e.to_string()),
        vault.clone().map_err(|e| e.to_string()),
        history.0.clone(),
        registry,
    );
    if let Some(file) = ela_workspace::SshConfigFile::from_home() {
        workspace = workspace.reading_ssh_config(file);
    }
    let workspace = Arc::new(workspace);
    let manager = workspace.manager().clone();

    let commands = commands();

    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(manager)
        .manage(workspace)
        .manage(files::Transfers::default())
        .manage(files::Drops::default())
        .manage(saved)
        .manage(vault)
        .manage(history)
        .invoke_handler(commands.invoke_handler())
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Destroyed => {
                window.app_handle().state::<Manager_>().close_all();
            }
            tauri::WindowEvent::DragDrop(drag) => {
                files::on_drag_drop(window, &window.state::<files::Drops>(), drag);
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running Ela");
}

#[cfg(test)]
mod no_secret_crosses_ipc;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use specta_typescript::Typescript;

    #[test]
    fn bindings_are_up_to_date() {
        let committed = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/bindings.ts");
        let dir = std::env::temp_dir().join(format!("ela-bindings-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let generated = dir.join("bindings.ts");
        super::commands()
            .export(Typescript::default(), &generated)
            .unwrap();
        let fresh = std::fs::read_to_string(&generated).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();

        if std::env::var_os("ELA_UPDATE_BINDINGS").is_some() {
            std::fs::write(&committed, &fresh).unwrap();
        }
        let current = std::fs::read_to_string(&committed).unwrap_or_default();
        assert!(
            current == fresh,
            "app/src/lib/bindings.ts is stale: run ELA_UPDATE_BINDINGS=1 cargo test -p ela-app bindings"
        );
    }

    // The browser preview offers the same option forms as the app without compiling Rust.
    #[test]
    fn bindings_for_the_preview_schemas_are_up_to_date() {
        let committed =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/dev/settings-schemas.json");
        let mut registry = super::ProtocolRegistry::new();
        registry
            .register(ela_ssh::SshProtocol::default())
            .register(ela_rdp::RdpProtocol::default());
        let schemas: serde_json::Map<String, serde_json::Value> =
            [ela_ssh::PROTOCOL_ID, ela_rdp::PROTOCOL_ID]
                .into_iter()
                .map(|id| (id.to_owned(), registry.get(id).unwrap().settings_schema()))
                .collect();
        let fresh = serde_json::to_string_pretty(&schemas).unwrap() + "\n";

        if std::env::var_os("ELA_UPDATE_BINDINGS").is_some() {
            std::fs::write(&committed, &fresh).unwrap();
        }
        let current = std::fs::read_to_string(&committed).unwrap_or_default();
        assert!(
            current == fresh,
            "app/src/dev/settings-schemas.json is stale: run ELA_UPDATE_BINDINGS=1 cargo test -p ela-app"
        );
    }
}
