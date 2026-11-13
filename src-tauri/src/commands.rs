//! Comandos Tauri expostos ao front. Nenhuma regra de negócio vive aqui: os comandos
//! apenas serializam o acesso e delegam para `ops`, `effects`, `snapshot`, `profiles` e
//! `system`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use tauri::State;

use crate::catalog::Catalog;
use crate::effects::{self, AlteracaoEfeito, EffectsBackend, ListaEfeitos, WindowsEfeitos};
use crate::error::{AppError, AppResult};
use crate::logger::Logger;
use crate::ops::{self, ApplyReport, ServiceDto};
use crate::profiles::{self, MudancaPrevista, Perfil, ProfileStore};
use crate::scm::{ServiceBackend, StartType, WindowsScm};
use crate::snapshot::{SnapshotInfo, SnapshotKind, SnapshotStore};
use crate::system::{self, RestorePointResult, SystemStats};

/// Estado compartilhado do app.
pub struct AppState {
    pub backend: Arc<dyn ServiceBackend>,
    pub efeitos: Arc<dyn EffectsBackend>,
    pub snapshots: SnapshotStore,
    pub perfis: ProfileStore,
    pub log: Logger,
    /// Serializa todas as operações de escrita.
    trava: Mutex<()>,
}

impl AppState {
    /// Estado real: SCM do Windows e dados em `%APPDATA%/WinTune`.
    pub fn novo() -> AppResult<Self> {
        let appdata = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or_else(|| {
                AppError::Outro("A variável de ambiente APPDATA não está definida.".into())
            })?;
        Ok(Self::com(
            Arc::new(WindowsScm),
            Arc::new(WindowsEfeitos),
            appdata.join("WinTune"),
        ))
    }

    pub fn com(
        backend: Arc<dyn ServiceBackend>,
        efeitos: Arc<dyn EffectsBackend>,
        pasta_dados: PathBuf,
    ) -> Self {
        Self {
            backend,
            snapshots: SnapshotStore::new(pasta_dados.join("snapshots"))
                .com_efeitos(efeitos.clone()),
            efeitos,
            perfis: ProfileStore::new(pasta_dados.join("profiles.json")),
            log: Logger::new(pasta_dados.join("logs")),
            trava: Mutex::new(()),
        }
    }

    fn travar(&self) -> MutexGuard<'_, ()> {
        self.trava.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Regra de segurança nº 2: cria o snapshot original na primeira execução.
    pub fn inicializar(&self) {
        let _guarda = self.travar();
        self.log.info(format!(
            "WinTune {} iniciado (administrador: {})",
            env!("CARGO_PKG_VERSION"),
            system::is_elevado()
        ));
        match self.snapshots.garantir_original(self.backend.as_ref()) {
            Ok(true) => self
                .log
                .info("Snapshot original criado (primeira execução)"),
            Ok(false) => {}
            Err(erro) => self.log.erro(format!(
                "Não foi possível criar o snapshot original: {erro}"
            )),
        }
    }

    /// Registra o resultado de um comando no log e o devolve.
    fn registrar<T>(&self, operacao: &str, resultado: AppResult<T>) -> AppResult<T> {
        match &resultado {
            Ok(_) => self.log.info(format!("OK {operacao}")),
            Err(erro) => self.log.erro(format!("ERRO {operacao}: {erro}")),
        }
        resultado
    }
}

#[tauri::command(async)]
pub fn list_services(state: State<'_, AppState>) -> AppResult<Vec<ServiceDto>> {
    let original = state.snapshots.original()?;
    ops::listar_dto(
        state.backend.as_ref(),
        Catalog::embutido()?,
        original.as_ref(),
    )
}

/// Liga/desliga um serviço. Devolve um aviso quando a mudança foi feita com ressalvas.
#[tauri::command(async)]
pub fn set_service_enabled(
    state: State<'_, AppState>,
    name: String,
    enabled: bool,
) -> AppResult<Option<String>> {
    let _guarda = state.travar();
    let resultado = (|| {
        // Sem snapshot original não há como voltar atrás: nada é alterado.
        state.snapshots.garantir_original(state.backend.as_ref())?;
        let original = state.snapshots.original()?;
        ops::definir_habilitado(
            state.backend.as_ref(),
            &state.log,
            original.as_ref(),
            &name,
            enabled,
        )
    })();
    state.registrar(
        &format!("set_service_enabled({name}, {enabled})"),
        resultado,
    )
}

#[tauri::command(async)]
pub fn set_start_type(
    state: State<'_, AppState>,
    name: String,
    start_type: StartType,
) -> AppResult<Option<String>> {
    let _guarda = state.travar();
    let resultado = (|| {
        state.snapshots.garantir_original(state.backend.as_ref())?;
        ops::definir_start_type(state.backend.as_ref(), &state.log, &name, start_type)
    })();
    state.registrar(
        &format!("set_start_type({name}, {start_type:?})"),
        resultado,
    )
}

#[tauri::command(async)]
pub fn list_profiles(state: State<'_, AppState>) -> AppResult<Vec<Perfil>> {
    profiles::todos(Catalog::embutido()?, &state.perfis)
}

/// Prévia (diff) do que um perfil alteraria agora. Não altera nada.
#[tauri::command(async)]
pub fn preview_profile(
    state: State<'_, AppState>,
    profile_id: String,
) -> AppResult<Vec<MudancaPrevista>> {
    let catalogo = Catalog::embutido()?;
    let perfil = profiles::buscar(catalogo, &state.perfis, &profile_id)?;
    Ok(perfil.previa(&state.backend.list()?, catalogo))
}

#[tauri::command(async)]
pub fn apply_profile(state: State<'_, AppState>, profile_id: String) -> AppResult<ApplyReport> {
    let _guarda = state.travar();
    let resultado = (|| {
        let perfil = profiles::buscar(Catalog::embutido()?, &state.perfis, &profile_id)?;
        ops::aplicar_lote_com_snapshot(
            state.backend.as_ref(),
            &state.log,
            &state.snapshots,
            &format!("Antes de aplicar o perfil {}", perfil.nome),
            &perfil.alteracoes(),
        )
    })();
    state.registrar(&format!("apply_profile({profile_id})"), resultado)
}

/// Salva a seleção atual (tudo que difere do estado original) como perfil personalizado.
#[tauri::command(async)]
pub fn save_custom_profile(state: State<'_, AppState>, nome: String) -> AppResult<Perfil> {
    let _guarda = state.travar();
    let resultado = (|| {
        let original = state.snapshots.original()?.ok_or_else(|| {
            AppError::Outro(
                "O backup original ainda não existe; não há como comparar a seleção atual.".into(),
            )
        })?;
        let itens = profiles::itens_da_selecao_atual(&original, &state.backend.list()?);
        state.perfis.salvar(&nome, itens)
    })();
    state.registrar(&format!("save_custom_profile({nome})"), resultado)
}

#[tauri::command(async)]
pub fn delete_profile(state: State<'_, AppState>, profile_id: String) -> AppResult<()> {
    let _guarda = state.travar();
    let resultado = state.perfis.excluir(&profile_id);
    state.registrar(&format!("delete_profile({profile_id})"), resultado)
}

#[tauri::command(async)]
pub fn create_snapshot(state: State<'_, AppState>) -> AppResult<SnapshotInfo> {
    let _guarda = state.travar();
    let resultado = (|| {
        state.snapshots.garantir_original(state.backend.as_ref())?;
        state.snapshots.criar(
            state.backend.as_ref(),
            SnapshotKind::Manual,
            "Backup manual",
        )
    })();
    state.registrar("create_snapshot", resultado)
}

#[tauri::command(async)]
pub fn list_snapshots(state: State<'_, AppState>) -> AppResult<Vec<SnapshotInfo>> {
    state.snapshots.listar()
}

#[tauri::command(async)]
pub fn restore_snapshot(state: State<'_, AppState>, id: String) -> AppResult<ApplyReport> {
    let _guarda = state.travar();
    let resultado = (|| {
        let snapshot = state.snapshots.carregar(&id)?;
        let motivo = format!("Antes de restaurar o backup {id}");
        let plano = snapshot.plano_de_restauracao(&state.backend.list()?);
        let mut relatorio = ops::aplicar_lote_com_snapshot(
            state.backend.as_ref(),
            &state.log,
            &state.snapshots,
            &motivo,
            &plano,
        )?;

        // Efeitos visuais: só se o backup os tiver. O backup automático já criado acima
        // (antes de mexer em serviços) também guardou os efeitos, então é reaproveitado.
        if let Some(salvos) = &snapshot.efeitos {
            let anterior = relatorio.snapshot_id.clone().filter(|id| {
                state
                    .snapshots
                    .carregar(id)
                    .is_ok_and(|s| s.efeitos.is_some())
            });
            match effects::restaurar(
                state.efeitos.as_ref(),
                state.backend.as_ref(),
                &state.log,
                &state.snapshots,
                salvos,
                &motivo,
                anterior,
            ) {
                Ok(r) => {
                    relatorio.sucessos.extend(r.sucessos);
                    relatorio.falhas.extend(r.falhas);
                    relatorio.ignorados.extend(r.ignorados);
                    relatorio.snapshot_id = relatorio.snapshot_id.or(r.snapshot_id);
                }
                // Os serviços já foram restaurados: a falha dos efeitos vira item do relatório.
                Err(erro) => relatorio.falhas.push(ops::ItemRelatorio {
                    name: "efeitos_visuais".into(),
                    display_name: "Efeitos visuais".into(),
                    detalhe: erro.to_string(),
                    aviso: None,
                }),
            }
        }
        Ok(relatorio)
    })();
    state.registrar(&format!("restore_snapshot({id})"), resultado)
}

#[tauri::command(async)]
pub fn delete_snapshot(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let _guarda = state.travar();
    let resultado = state.snapshots.excluir(&id);
    state.registrar(&format!("delete_snapshot({id})"), resultado)
}

#[tauri::command(async)]
pub fn create_restore_point(state: State<'_, AppState>) -> AppResult<RestorePointResult> {
    let _guarda = state.travar();
    let resultado = system::criar_ponto_de_restauracao();
    state.registrar("create_restore_point", resultado)
}

#[tauri::command(async)]
pub fn list_effects(state: State<'_, AppState>) -> AppResult<ListaEfeitos> {
    let original = state.snapshots.original()?;
    Ok(effects::listar_dto(
        state.efeitos.as_ref(),
        original.as_ref(),
    ))
}

/// Liga/desliga um efeito visual. Devolve um aviso se precisar sair e entrar na conta.
#[tauri::command(async)]
pub fn set_effect(
    state: State<'_, AppState>,
    id: String,
    ligado: bool,
) -> AppResult<Option<String>> {
    let _guarda = state.travar();
    let resultado = effects::definir(
        state.efeitos.as_ref(),
        state.backend.as_ref(),
        &state.log,
        &state.snapshots,
        &id,
        ligado,
    );
    state.registrar(&format!("set_effect({id}, {ligado})"), resultado)
}

/// Aplica várias alterações de efeitos (sempre com backup automático antes).
#[tauri::command(async)]
pub fn apply_effects(
    state: State<'_, AppState>,
    alteracoes: Vec<AlteracaoEfeito>,
    motivo: String,
) -> AppResult<ApplyReport> {
    let _guarda = state.travar();
    // O motivo vem do front e vai para o arquivo de backup: limita o tamanho.
    let motivo: String = motivo.chars().take(120).collect();
    let resultado = effects::aplicar_lote(
        state.efeitos.as_ref(),
        state.backend.as_ref(),
        &state.log,
        &state.snapshots,
        &format!("Antes de: {motivo}"),
        &alteracoes,
        None,
    );
    state.registrar(
        &format!("apply_effects({motivo}, {} itens)", alteracoes.len()),
        resultado,
    )
}

#[tauri::command(async)]
pub fn system_stats() -> SystemStats {
    system::stats()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::mock::MockEfeitos;
    use crate::scm::mock::MockBackend;
    use crate::snapshot::ID_ORIGINAL;

    #[test]
    fn inicializar_cria_o_snapshot_original_e_registra_em_log() {
        let dir = tempfile::tempdir().unwrap();
        let backend = MockBackend::com(MockBackend::new(), "DiagTrack", StartType::Automatic, true);
        let state = AppState::com(
            Arc::new(backend),
            Arc::new(MockEfeitos::new()),
            dir.path().join("WinTune"),
        );

        state.inicializar();
        let original = state.snapshots.original().unwrap().unwrap();
        assert_eq!(original.id, ID_ORIGINAL);
        assert_eq!(original.servicos.len(), 1);

        // Segunda execução não recria nem altera o original.
        state.inicializar();
        assert_eq!(state.snapshots.original().unwrap().unwrap(), original);

        let logs: Vec<_> = std::fs::read_dir(dir.path().join("WinTune").join("logs"))
            .unwrap()
            .collect();
        assert_eq!(logs.len(), 1);
        let conteudo = std::fs::read_to_string(logs[0].as_ref().unwrap().path()).unwrap();
        assert!(conteudo.contains("Snapshot original criado"));
    }

    #[test]
    fn registrar_grava_sucesso_e_erro() {
        let dir = tempfile::tempdir().unwrap();
        let state = AppState::com(
            Arc::new(MockBackend::new()),
            Arc::new(MockEfeitos::new()),
            dir.path().to_path_buf(),
        );
        assert!(state.registrar("operacao_a", Ok(1)).is_ok());
        assert!(
            state
                .registrar::<()>("operacao_b", Err(AppError::Outro("deu ruim".into())))
                .is_err()
        );

        let arquivo = std::fs::read_dir(dir.path().join("logs"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let conteudo = std::fs::read_to_string(arquivo).unwrap();
        assert!(conteudo.contains("[INFO] OK operacao_a"));
        assert!(conteudo.contains("[ERRO] ERRO operacao_b: deu ruim"));
    }
}
