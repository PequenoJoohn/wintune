//! Motor de operações: todo caminho de escrita passa por aqui, onde o `guard` é
//! consultado, o resultado é registrado em log e os lotes ganham snapshot automático.

use std::collections::HashMap;

use serde::Serialize;

use crate::catalog::{Catalog, Risco};
use crate::error::{AppError, AppResult};
use crate::guard::{self, Protecao};
use crate::logger::Logger;
use crate::scm::{ServiceBackend, ServiceEntry, ServiceStatus, StartType};
use crate::snapshot::{Snapshot, SnapshotKind, SnapshotStore};

/// Uma alteração desejada em um serviço.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alteracao {
    pub name: String,
    pub start_type: StartType,
    /// Depois de alterar, iniciar o serviço se ele estiver parado (usado em restaurações).
    pub garantir_rodando: bool,
}

/// O que acontecerá com uma alteração planejada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Situacao {
    Alterar,
    SemMudanca,
    Ausente,
    Protegido,
}

/// Alteração já confrontada com o estado atual do sistema.
#[derive(Debug, Clone)]
pub struct Planejado {
    pub alteracao: Alteracao,
    pub atual: Option<ServiceEntry>,
    pub situacao: Situacao,
    pub motivo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemRelatorio {
    pub name: String,
    pub display_name: String,
    /// Para sucessos: o que mudou. Para falhas/ignorados: o motivo.
    pub detalhe: String,
    /// Ressalva em um sucesso (ex.: desativado, mas só para de rodar após reiniciar).
    pub aviso: Option<String>,
}

/// Relatório de uma operação em lote.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ApplyReport {
    pub sucessos: Vec<ItemRelatorio>,
    pub falhas: Vec<ItemRelatorio>,
    pub ignorados: Vec<ItemRelatorio>,
    /// Snapshot criado automaticamente antes do lote (se algo precisou mudar).
    pub snapshot_id: Option<String>,
}

/// Serviço como exibido no front.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ServiceDto {
    pub name: String,
    pub display_name: String,
    pub status: ServiceStatus,
    pub start_type: StartType,
    pub categoria: Option<String>,
    pub risco: Option<Risco>,
    pub descricao: Option<String>,
    pub recomendado: Option<StartType>,
    /// Não pode ser alterado: crítico, por usuário ou negado pelo Windows.
    pub protegido: bool,
    /// Pode mudar de tipo de inicialização, mas nunca ser desativado (ex.: wuauserv).
    pub restrito: bool,
    pub motivo_protecao: Option<String>,
    /// Tipo de inicialização no snapshot original.
    pub original_start_type: Option<StartType>,
    /// O tipo de inicialização atual difere do original.
    pub alterado: bool,
}

/// Valida uma alteração contra o guard e contra o estado lido do SCM.
fn validar(atual: &ServiceEntry, alvo: StartType) -> AppResult<()> {
    guard::validar_start_type(&atual.name, alvo)?;
    if alvo == StartType::Disabled {
        guard::validar_parada(&atual.name)?;
    }
    if atual.per_user {
        return Err(AppError::ServicoPorUsuario(atual.name.clone()));
    }
    if atual.acesso_negado {
        return Err(AppError::AcessoNegado(atual.name.clone()));
    }
    Ok(())
}

/// Confronta as alterações desejadas com o estado atual, sem alterar nada.
pub fn planejar(alteracoes: &[Alteracao], atuais: &[ServiceEntry]) -> Vec<Planejado> {
    let indice: HashMap<String, &ServiceEntry> =
        atuais.iter().map(|s| (s.name.to_lowercase(), s)).collect();

    alteracoes
        .iter()
        .map(|alteracao| {
            let Some(atual) = indice.get(&alteracao.name.to_lowercase()).copied() else {
                // Mesmo ausente, um nome protegido é reportado como protegido.
                let (situacao, motivo) =
                    match guard::validar_start_type(&alteracao.name, alteracao.start_type) {
                        Err(erro) => (Situacao::Protegido, erro.to_string()),
                        Ok(()) => (
                            Situacao::Ausente,
                            "Não existe neste computador.".to_string(),
                        ),
                    };
                return Planejado {
                    alteracao: alteracao.clone(),
                    atual: None,
                    situacao,
                    motivo: Some(motivo),
                };
            };

            let (situacao, motivo) = if let Err(erro) = validar(atual, alteracao.start_type) {
                (Situacao::Protegido, Some(erro.to_string()))
            } else if atual.start_type == alteracao.start_type
                && !(alteracao.start_type == StartType::Disabled && atual.status.is_ativo())
            {
                (
                    Situacao::SemMudanca,
                    Some(format!("Já está em {}.", atual.start_type.rotulo())),
                )
            } else {
                (Situacao::Alterar, None)
            };

            Planejado {
                alteracao: alteracao.clone(),
                atual: Some(atual.clone()),
                situacao,
                motivo,
            }
        })
        .collect()
}

/// Aplica uma alteração já lida do SCM. Devolve um aviso quando a mudança foi feita
/// com ressalvas (ex.: o serviço não pôde ser parado agora).
fn executar(
    backend: &dyn ServiceBackend,
    log: &Logger,
    atual: &ServiceEntry,
    alvo: StartType,
    garantir_rodando: bool,
) -> AppResult<Option<String>> {
    let name = atual.name.as_str();
    if let Err(erro) = validar(atual, alvo) {
        log.aviso(format!("BLOQUEADO {name} -> {alvo:?}: {erro}"));
        return Err(erro);
    }

    if atual.start_type != alvo {
        if let Err(erro) = backend.set_start_type(name, alvo) {
            log.erro(format!("FALHA start type {name} -> {alvo:?}: {erro}"));
            return Err(erro);
        }
        log.info(format!(
            "Start type de {name}: {:?} -> {alvo:?}",
            atual.start_type
        ));
    }

    let mut aviso = None;
    if alvo == StartType::Disabled {
        if atual.status.is_ativo() {
            match backend.stop(name) {
                Ok(()) => log.info(format!("Serviço {name} parado")),
                Err(erro) => {
                    log.aviso(format!("{name} desativado, mas não parou: {erro}"));
                    aviso = Some(format!(
                        "Desativado, mas continua em execução até reiniciar o computador. ({erro})"
                    ));
                }
            }
        }
    } else if garantir_rodando && !atual.status.is_ativo() {
        match guard::validar_inicio(name).and_then(|()| backend.start(name)) {
            Ok(()) => log.info(format!("Serviço {name} iniciado")),
            Err(erro) => {
                log.aviso(format!("{name} reativado, mas não iniciou: {erro}"));
                aviso = Some(format!(
                    "Tipo de inicialização restaurado, mas o serviço não pôde ser iniciado agora. ({erro})"
                ));
            }
        }
    }
    Ok(aviso)
}

/// Altera o tipo de inicialização de um único serviço.
/// Definir como Desativado também para o serviço.
pub fn definir_start_type(
    backend: &dyn ServiceBackend,
    log: &Logger,
    name: &str,
    alvo: StartType,
) -> AppResult<Option<String>> {
    // O guard vem antes de qualquer consulta ao SCM.
    guard::validar_start_type(name, alvo)?;
    let atual = backend.get(name)?;
    executar(backend, log, &atual, alvo, false)
}

/// Liga ou desliga um serviço.
///
/// - Desligar: define como Desativado e para o serviço.
/// - Ligar: volta ao tipo de inicialização do snapshot original (ou Manual, se o original
///   já era Desativado/desconhecido) e inicia o serviço se ele rodava originalmente.
pub fn definir_habilitado(
    backend: &dyn ServiceBackend,
    log: &Logger,
    original: Option<&Snapshot>,
    name: &str,
    habilitar: bool,
) -> AppResult<Option<String>> {
    if !habilitar {
        return definir_start_type(backend, log, name, StartType::Disabled);
    }

    let salvo = original.and_then(|s| s.servico(name));
    let alvo = match salvo.map(|s| s.start_type) {
        Some(tipo) if !matches!(tipo, StartType::Disabled | StartType::Unknown) => tipo,
        _ => StartType::Manual,
    };
    let iniciar = salvo.is_some_and(|s| s.rodando) || alvo.is_automatico();

    guard::validar_start_type(name, alvo)?;
    let atual = backend.get(name)?;
    executar(backend, log, &atual, alvo, iniciar)
}

/// Aplica um lote de alterações e devolve o relatório. Não cria snapshot:
/// use [`aplicar_lote_com_snapshot`] a partir dos comandos.
pub fn aplicar_lote(
    backend: &dyn ServiceBackend,
    log: &Logger,
    planejados: &[Planejado],
) -> ApplyReport {
    let mut relatorio = ApplyReport::default();

    for planejado in planejados {
        let alvo = planejado.alteracao.start_type;
        let (name, display_name) = match &planejado.atual {
            Some(atual) => (atual.name.clone(), atual.display_name.clone()),
            None => (
                planejado.alteracao.name.clone(),
                planejado.alteracao.name.clone(),
            ),
        };
        let item = |detalhe: String, aviso: Option<String>| ItemRelatorio {
            name: name.clone(),
            display_name: display_name.clone(),
            detalhe,
            aviso,
        };

        match (&planejado.atual, planejado.situacao) {
            (Some(atual), Situacao::Alterar) => {
                match executar(
                    backend,
                    log,
                    atual,
                    alvo,
                    planejado.alteracao.garantir_rodando,
                ) {
                    Ok(aviso) => relatorio.sucessos.push(item(
                        format!("{} → {}", atual.start_type.rotulo(), alvo.rotulo()),
                        aviso,
                    )),
                    Err(erro) => relatorio.falhas.push(item(erro.to_string(), None)),
                }
            }
            _ => relatorio.ignorados.push(item(
                planejado
                    .motivo
                    .clone()
                    .unwrap_or_else(|| "Nenhuma alteração necessária.".to_string()),
                None,
            )),
        }
    }

    log.info(format!(
        "Lote concluído: {} alterados, {} falharam, {} ignorados",
        relatorio.sucessos.len(),
        relatorio.falhas.len(),
        relatorio.ignorados.len()
    ));
    relatorio
}

/// Regra de segurança nº 1: garante o snapshot original, cria um snapshot automático
/// e só então aplica o lote. Se o snapshot falhar, nada é alterado.
pub fn aplicar_lote_com_snapshot(
    backend: &dyn ServiceBackend,
    log: &Logger,
    store: &SnapshotStore,
    motivo: &str,
    alteracoes: &[Alteracao],
) -> AppResult<ApplyReport> {
    store.garantir_original(backend)?;

    let atuais = backend.list()?;
    let planejados = planejar(alteracoes, &atuais);

    let mut snapshot_id = None;
    if planejados.iter().any(|p| p.situacao == Situacao::Alterar) {
        let info = store
            .criar(backend, SnapshotKind::Automatico, motivo)
            .inspect_err(|erro| {
                log.erro(format!(
                    "Lote cancelado ({motivo}): snapshot automático falhou: {erro}"
                ));
            })?;
        log.info(format!("Snapshot automático {} ({motivo})", info.id));
        snapshot_id = Some(info.id);
    }

    let mut relatorio = aplicar_lote(backend, log, &planejados);
    relatorio.snapshot_id = snapshot_id;
    Ok(relatorio)
}

/// Lista os serviços enriquecidos com catálogo, proteção e estado original.
pub fn listar_dto(
    backend: &dyn ServiceBackend,
    catalogo: &Catalog,
    original: Option<&Snapshot>,
) -> AppResult<Vec<ServiceDto>> {
    let originais: HashMap<String, StartType> = original
        .map(|s| {
            s.servicos
                .iter()
                .map(|x| (x.name.to_lowercase(), x.start_type))
                .collect()
        })
        .unwrap_or_default();

    Ok(backend
        .list()?
        .into_iter()
        .map(|servico| {
            let protecao = if servico.per_user {
                Protecao::PorUsuario
            } else {
                guard::classificar(&servico.name)
            };
            let motivo_protecao = match guard::motivo(protecao) {
                Some(motivo) => Some(motivo.to_string()),
                None if servico.acesso_negado => Some(
                    "O Windows negou acesso a este serviço (protegido pelo sistema).".to_string(),
                ),
                None => None,
            };
            let entrada = catalogo.get(&servico.name);
            let original_start_type = originais.get(&servico.name.to_lowercase()).copied();

            ServiceDto {
                categoria: entrada.map(|e| e.categoria.clone()),
                risco: entrada.map(|e| e.risco),
                descricao: entrada.map(|e| e.descricao.clone()),
                recomendado: entrada.map(|e| e.recomendado),
                protegido: servico.acesso_negado
                    || matches!(protecao, Protecao::Bloqueado | Protecao::PorUsuario),
                restrito: protecao == Protecao::Restrito,
                motivo_protecao,
                original_start_type,
                alterado: original_start_type
                    .is_some_and(|o| o != StartType::Unknown && o != servico.start_type),
                name: servico.name,
                display_name: servico.display_name,
                status: servico.status,
                start_type: servico.start_type,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::BLOQUEADOS;
    use crate::scm::mock::MockBackend;

    fn mock() -> MockBackend {
        let m = MockBackend::new();
        let m = MockBackend::com(m, "DiagTrack", StartType::Automatic, true);
        let m = MockBackend::com(m, "WSearch", StartType::AutomaticDelayed, true);
        let m = MockBackend::com(m, "Fax", StartType::Manual, false);
        let m = MockBackend::com(m, "RemoteRegistry", StartType::Disabled, false);
        let m = MockBackend::com(m, "RpcSs", StartType::Automatic, true);
        let m = MockBackend::com(m, "wuauserv", StartType::Manual, false);
        let mut m = MockBackend::com(m, "WaaSProtegido", StartType::Manual, true);
        m.inserir(ServiceEntry {
            name: "CDPUserSvc_3f2a1".into(),
            display_name: "Plataforma de Dispositivos Conectados".into(),
            status: ServiceStatus::Running,
            start_type: StartType::Automatic,
            per_user: true,
            acesso_negado: false,
        });
        m.negar_acesso("WaaSProtegido");
        m
    }

    fn store() -> (tempfile::TempDir, SnapshotStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::new(dir.path().join("snapshots"));
        (dir, store)
    }

    fn desativar(name: &str) -> Alteracao {
        Alteracao {
            name: name.into(),
            start_type: StartType::Disabled,
            garantir_rodando: false,
        }
    }

    fn log() -> Logger {
        Logger::desativado()
    }

    #[test]
    fn desligar_desativa_e_para_o_servico() {
        let backend = mock();
        let aviso = definir_habilitado(&backend, &log(), None, "DiagTrack", false).unwrap();
        assert!(aviso.is_none());

        let servico = backend.get("DiagTrack").unwrap();
        assert_eq!(servico.start_type, StartType::Disabled);
        assert_eq!(servico.status, ServiceStatus::Stopped);
    }

    #[test]
    fn ligar_volta_ao_tipo_original_e_reinicia() {
        let (_dir, store) = store();
        let backend = mock();
        store.garantir_original(&backend).unwrap();
        let original = store.original().unwrap();

        definir_habilitado(&backend, &log(), original.as_ref(), "WSearch", false).unwrap();
        assert_eq!(
            backend.get("WSearch").unwrap().start_type,
            StartType::Disabled
        );

        definir_habilitado(&backend, &log(), original.as_ref(), "WSearch", true).unwrap();
        let servico = backend.get("WSearch").unwrap();
        assert_eq!(servico.start_type, StartType::AutomaticDelayed);
        assert_eq!(servico.status, ServiceStatus::Running);
    }

    #[test]
    fn ligar_servico_originalmente_desativado_usa_manual_sem_iniciar() {
        let (_dir, store) = store();
        let backend = mock();
        store.garantir_original(&backend).unwrap();
        let original = store.original().unwrap();

        definir_habilitado(&backend, &log(), original.as_ref(), "RemoteRegistry", true).unwrap();
        let servico = backend.get("RemoteRegistry").unwrap();
        assert_eq!(servico.start_type, StartType::Manual);
        assert_eq!(servico.status, ServiceStatus::Stopped);
    }

    #[test]
    fn backend_rejeita_servicos_bloqueados_mesmo_se_o_front_pedir() {
        let backend = mock();
        for name in BLOQUEADOS {
            assert!(matches!(
                definir_habilitado(&backend, &log(), None, name, false),
                Err(AppError::ServicoBloqueado(_))
            ));
            for tipo in [StartType::Manual, StartType::Disabled, StartType::Automatic] {
                assert!(matches!(
                    definir_start_type(&backend, &log(), name, tipo),
                    Err(AppError::ServicoBloqueado(_))
                ));
            }
        }
        // Nenhuma chamada de escrita chegou ao SCM.
        assert!(backend.chamadas().is_empty());
        assert_eq!(backend.get("RpcSs").unwrap().status, ServiceStatus::Running);
    }

    #[test]
    fn wuauserv_aceita_manual_mas_nunca_desativado() {
        let backend = mock();
        assert!(matches!(
            definir_start_type(&backend, &log(), "wuauserv", StartType::Disabled),
            Err(AppError::ServicoRestrito(_))
        ));
        assert!(matches!(
            definir_habilitado(&backend, &log(), None, "wuauserv", false),
            Err(AppError::ServicoRestrito(_))
        ));
        assert!(backend.chamadas().is_empty());

        definir_start_type(&backend, &log(), "wuauserv", StartType::Automatic).unwrap();
        definir_start_type(&backend, &log(), "wuauserv", StartType::Manual).unwrap();
        assert_eq!(
            backend.get("wuauserv").unwrap().start_type,
            StartType::Manual
        );
    }

    #[test]
    fn servico_por_usuario_e_acesso_negado_sao_rejeitados_sem_escrita() {
        let backend = mock();
        assert!(matches!(
            definir_habilitado(&backend, &log(), None, "CDPUserSvc_3f2a1", false),
            Err(AppError::ServicoPorUsuario(_))
        ));
        assert!(matches!(
            definir_habilitado(&backend, &log(), None, "WaaSProtegido", false),
            Err(AppError::AcessoNegado(_))
        ));
        assert!(matches!(
            definir_habilitado(&backend, &log(), None, "NaoExiste", false),
            Err(AppError::ServicoNaoEncontrado(_))
        ));
        assert!(backend.chamadas().is_empty());
    }

    #[test]
    fn falha_ao_parar_vira_aviso_e_nao_erro() {
        let backend = mock();
        backend.recusar_parada("DiagTrack");
        let aviso = definir_habilitado(&backend, &log(), None, "DiagTrack", false).unwrap();
        assert!(aviso.unwrap().contains("reiniciar"));
        assert_eq!(
            backend.get("DiagTrack").unwrap().start_type,
            StartType::Disabled
        );
    }

    #[test]
    fn planejar_classifica_cada_alteracao() {
        let backend = mock();
        let alteracoes: Vec<_> = [
            "DiagTrack",
            "RemoteRegistry",
            "NaoExiste",
            "RpcSs",
            "wuauserv",
            "CDPUserSvc_3f2a1",
            "WaaSProtegido",
            "WinDefend", // bloqueado e ausente do mock
        ]
        .into_iter()
        .map(desativar)
        .collect();

        let plano = planejar(&alteracoes, &backend.list().unwrap());
        let situacoes: Vec<_> = plano.iter().map(|p| p.situacao).collect();
        assert_eq!(
            situacoes,
            vec![
                Situacao::Alterar,
                Situacao::SemMudanca,
                Situacao::Ausente,
                Situacao::Protegido,
                Situacao::Protegido,
                Situacao::Protegido,
                Situacao::Protegido,
                Situacao::Protegido,
            ]
        );
        assert!(plano.iter().skip(1).all(|p| p.motivo.is_some()));
        assert!(backend.chamadas().is_empty(), "planejar não altera nada");
    }

    #[test]
    fn servico_desativado_mas_rodando_ainda_precisa_ser_parado() {
        let backend = MockBackend::com(MockBackend::new(), "Zumbi", StartType::Disabled, true);
        let plano = planejar(&[desativar("Zumbi")], &backend.list().unwrap());
        assert_eq!(plano[0].situacao, Situacao::Alterar);
    }

    #[test]
    fn lote_cria_snapshot_antes_de_alterar_e_relata_tudo() {
        let (_dir, store) = store();
        let backend = mock();
        let alteracoes: Vec<_> = [
            "DiagTrack",
            "WSearch",
            "RemoteRegistry",
            "NaoExiste",
            "RpcSs",
            "WaaSProtegido",
        ]
        .into_iter()
        .map(desativar)
        .collect();

        let relatorio =
            aplicar_lote_com_snapshot(&backend, &log(), &store, "Perfil de teste", &alteracoes)
                .unwrap();

        let nomes = |itens: &[ItemRelatorio]| -> Vec<String> {
            itens.iter().map(|i| i.name.clone()).collect()
        };
        assert_eq!(nomes(&relatorio.sucessos), vec!["DiagTrack", "WSearch"]);
        assert!(relatorio.falhas.is_empty());
        assert_eq!(
            nomes(&relatorio.ignorados),
            vec!["RemoteRegistry", "NaoExiste", "RpcSs", "WaaSProtegido"]
        );
        assert_eq!(relatorio.sucessos[0].detalhe, "Automático → Desativado");

        // O snapshot automático guarda o estado ANTERIOR ao lote...
        let snapshot = store
            .carregar(relatorio.snapshot_id.as_deref().unwrap())
            .unwrap();
        assert_eq!(snapshot.tipo, SnapshotKind::Automatico);
        assert_eq!(snapshot.motivo, "Perfil de teste");
        assert_eq!(
            snapshot.servico("DiagTrack").unwrap().start_type,
            StartType::Automatic
        );
        // ...e o original também foi garantido.
        assert!(store.original().unwrap().is_some());

        // Serviço crítico intacto.
        assert_eq!(
            backend.get("RpcSs").unwrap().start_type,
            StartType::Automatic
        );
    }

    #[test]
    fn lote_sem_mudancas_nao_cria_snapshot_automatico() {
        let (_dir, store) = store();
        let backend = mock();
        let relatorio = aplicar_lote_com_snapshot(
            &backend,
            &log(),
            &store,
            "nada",
            &[desativar("RemoteRegistry"), desativar("RpcSs")],
        )
        .unwrap();
        assert!(relatorio.snapshot_id.is_none());
        assert!(relatorio.sucessos.is_empty());
        assert_eq!(relatorio.ignorados.len(), 2);
        assert!(backend.chamadas().is_empty());
    }

    #[test]
    fn lote_nao_altera_nada_se_o_snapshot_falhar() {
        let dir = tempfile::tempdir().unwrap();
        // Um arquivo no lugar da pasta de snapshots faz a gravação falhar.
        let caminho = dir.path().join("snapshots");
        std::fs::write(&caminho, "não sou uma pasta").unwrap();
        let store = SnapshotStore::new(&caminho);
        let backend = mock();

        let resultado =
            aplicar_lote_com_snapshot(&backend, &log(), &store, "x", &[desativar("DiagTrack")]);
        assert!(resultado.is_err());
        assert!(backend.chamadas().is_empty());
        assert_eq!(
            backend.get("DiagTrack").unwrap().start_type,
            StartType::Automatic
        );
    }

    #[test]
    fn falha_do_scm_durante_o_lote_vira_item_de_falha() {
        let (_dir, store) = store();
        let backend = mock();
        let atuais = backend.list().unwrap();
        // O acesso passa a ser negado depois do planejamento (ex.: serviço protegido).
        let planejados = planejar(&[desativar("DiagTrack"), desativar("Fax")], &atuais);
        backend.negar_acesso("DiagTrack");

        let relatorio = aplicar_lote(&backend, &log(), &planejados);
        assert_eq!(relatorio.falhas.len(), 1);
        assert_eq!(relatorio.falhas[0].name, "DiagTrack");
        assert!(relatorio.falhas[0].detalhe.contains("Acesso negado"));
        assert_eq!(relatorio.sucessos.len(), 1);
        drop(store);
    }

    #[test]
    fn restaurar_snapshot_desfaz_o_lote() {
        let (_dir, store) = store();
        let backend = mock();
        store.garantir_original(&backend).unwrap();

        let alteracoes: Vec<_> = ["DiagTrack", "WSearch", "Fax"]
            .into_iter()
            .map(desativar)
            .collect();
        aplicar_lote_com_snapshot(&backend, &log(), &store, "perfil", &alteracoes).unwrap();
        assert_eq!(backend.get("Fax").unwrap().start_type, StartType::Disabled);

        let original = store.original().unwrap().unwrap();
        let plano = original.plano_de_restauracao(&backend.list().unwrap());
        let relatorio =
            aplicar_lote_com_snapshot(&backend, &log(), &store, "restauração", &plano).unwrap();
        assert_eq!(relatorio.sucessos.len(), 3);
        assert!(relatorio.falhas.is_empty());

        let diag = backend.get("DiagTrack").unwrap();
        assert_eq!(diag.start_type, StartType::Automatic);
        assert_eq!(diag.status, ServiceStatus::Running);
        let wsearch = backend.get("WSearch").unwrap();
        assert_eq!(wsearch.start_type, StartType::AutomaticDelayed);
        assert_eq!(wsearch.status, ServiceStatus::Running);
        let fax = backend.get("Fax").unwrap();
        assert_eq!(fax.start_type, StartType::Manual);
        assert_eq!(fax.status, ServiceStatus::Stopped);

        // Restaurar de novo não tem mais nada a fazer.
        let plano = original.plano_de_restauracao(&backend.list().unwrap());
        assert!(plano.is_empty());
    }

    #[test]
    fn dto_marca_protecao_catalogo_e_alteracoes() {
        let (_dir, store) = store();
        let backend = mock();
        store.garantir_original(&backend).unwrap();
        let original = store.original().unwrap();
        definir_habilitado(&backend, &log(), original.as_ref(), "DiagTrack", false).unwrap();

        let catalogo = Catalog::embutido().unwrap();
        let lista = listar_dto(&backend, catalogo, original.as_ref()).unwrap();
        let buscar = |name: &str| lista.iter().find(|s| s.name == name).unwrap();

        let diag = buscar("DiagTrack");
        assert_eq!(diag.categoria.as_deref(), Some("Telemetria"));
        assert_eq!(diag.risco, Some(Risco::Seguro));
        assert!(diag.descricao.is_some());
        assert!(!diag.protegido);
        assert!(diag.alterado);
        assert_eq!(diag.original_start_type, Some(StartType::Automatic));

        let fax = buscar("Fax");
        assert!(!fax.alterado);

        let rpc = buscar("RpcSs");
        assert!(rpc.protegido);
        assert!(rpc.motivo_protecao.is_some());
        assert!(rpc.categoria.is_none());

        let wu = buscar("wuauserv");
        assert!(!wu.protegido);
        assert!(wu.restrito);

        let usuario = buscar("CDPUserSvc_3f2a1");
        assert!(usuario.protegido);

        let negado = buscar("WaaSProtegido");
        assert!(negado.protegido);
        assert!(negado.motivo_protecao.as_deref().unwrap().contains("negou"));
    }
}
