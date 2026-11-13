// Regra do projeto: nada de unwrap()/expect() em código de produção.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod catalog;
pub mod commands;
pub mod effects;
pub mod error;
pub mod guard;
pub mod logger;
pub mod ops;
pub mod profiles;
pub mod scm;
pub mod snapshot;
pub mod system;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = match AppState::novo() {
        Ok(state) => state,
        Err(erro) => {
            eprintln!("Falha ao iniciar o WinTune: {erro}");
            std::process::exit(1);
        }
    };
    // Primeira execução: grava o snapshot "original" antes de qualquer interação.
    state.inicializar();

    let resultado = tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::list_services,
            commands::set_service_enabled,
            commands::set_start_type,
            commands::list_profiles,
            commands::preview_profile,
            commands::apply_profile,
            commands::save_custom_profile,
            commands::delete_profile,
            commands::create_snapshot,
            commands::list_snapshots,
            commands::restore_snapshot,
            commands::delete_snapshot,
            commands::create_restore_point,
            commands::list_effects,
            commands::set_effect,
            commands::apply_effects,
            commands::system_stats,
        ])
        .run(tauri::generate_context!());

    if let Err(erro) = resultado {
        eprintln!("Falha ao iniciar o WinTune: {erro}");
        std::process::exit(1);
    }
}
