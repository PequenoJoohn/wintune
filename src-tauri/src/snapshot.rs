//! Snapshots do estado dos serviços em `%APPDATA%/WinTune/snapshots/<id>.json`.
//!
//! O snapshot `original` é criado na primeira execução e nunca é sobrescrito nem excluído.

use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::ops::Alteracao;
use crate::scm::{ServiceBackend, ServiceEntry, StartType};

/// Identificador fixo do snapshot criado na primeira execução.
pub const ID_ORIGINAL: &str = "original";
const VERSAO_FORMATO: u32 = 1;
/// Quantidade máxima de snapshots automáticos mantidos em disco.
pub const MAX_AUTOMATICOS: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SnapshotKind {
    /// Estado da primeira execução. Nunca é sobrescrito.
    Original,
    /// Criado automaticamente antes de uma alteração em lote.
    Automatico,
    /// Criado a pedido do usuário.
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotService {
    pub name: String,
    pub display_name: String,
    pub start_type: StartType,
    pub rodando: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub versao: u32,
    pub id: String,
    pub tipo: SnapshotKind,
    pub criado_em: DateTime<Local>,
    pub motivo: String,
    pub servicos: Vec<SnapshotService>,
}

/// Resumo de um snapshot, para listagem no front.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub id: String,
    pub tipo: SnapshotKind,
    pub criado_em: DateTime<Local>,
    pub motivo: String,
    pub total_servicos: usize,
}

impl Snapshot {
    /// Monta um snapshot a partir do estado atual dos serviços.
    pub fn capturar(
        id: String,
        tipo: SnapshotKind,
        motivo: impl Into<String>,
        servicos: &[ServiceEntry],
    ) -> Self {
        Self {
            versao: VERSAO_FORMATO,
            id,
            tipo,
            criado_em: Local::now(),
            motivo: motivo.into(),
            servicos: servicos
                .iter()
                .map(|s| SnapshotService {
                    name: s.name.clone(),
                    display_name: s.display_name.clone(),
                    start_type: s.start_type,
                    rodando: s.status.is_ativo(),
                })
                .collect(),
        }
    }

    pub fn info(&self) -> SnapshotInfo {
        SnapshotInfo {
            id: self.id.clone(),
            tipo: self.tipo,
            criado_em: self.criado_em,
            motivo: self.motivo.clone(),
            total_servicos: self.servicos.len(),
        }
    }

    pub fn servico(&self, name: &str) -> Option<&SnapshotService> {
        self.servicos
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// Alterações necessárias para voltar do estado `atuais` ao estado deste snapshot.
    ///
    /// Só entram serviços cujo tipo de inicialização mudou. Serviços que não existem mais
    /// ou cujo tipo não pôde ser lido são deixados como estão.
    pub fn plano_de_restauracao(&self, atuais: &[ServiceEntry]) -> Vec<Alteracao> {
        self.servicos
            .iter()
            .filter(|salvo| salvo.start_type != StartType::Unknown)
            .filter_map(|salvo| {
                let atual = atuais
                    .iter()
                    .find(|a| a.name.eq_ignore_ascii_case(&salvo.name))?;
                (atual.start_type != salvo.start_type).then(|| Alteracao {
                    name: atual.name.clone(),
                    start_type: salvo.start_type,
                    garantir_rodando: salvo.rodando,
                })
            })
            .collect()
    }
}

/// Armazena snapshots em uma pasta.
#[derive(Debug, Clone)]
pub struct SnapshotStore {
    dir: PathBuf,
}

impl SnapshotStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn caminho(&self, id: &str) -> AppResult<PathBuf> {
        // O id vem do front: só aceita caracteres seguros para nome de arquivo.
        let valido = !id.is_empty()
            && id.len() <= 64
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !valido {
            return Err(AppError::Invalido(
                "Identificador de backup inválido.".into(),
            ));
        }
        Ok(self.dir.join(format!("{id}.json")))
    }

    fn garantir_dir(&self) -> AppResult<()> {
        fs::create_dir_all(&self.dir).map_err(|e| AppError::io("criar a pasta de backups", e))
    }

    fn novo_id(&self) -> String {
        let base = Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
        let mut id = base.clone();
        let mut sufixo = 1;
        while self.dir.join(format!("{id}.json")).exists() {
            sufixo += 1;
            id = format!("{base}-{sufixo}");
        }
        id
    }

    /// Grava sem jamais sobrescrever um arquivo existente.
    fn gravar_novo(&self, snapshot: &Snapshot) -> AppResult<()> {
        self.garantir_dir()?;
        let caminho = self.caminho(&snapshot.id)?;
        let json =
            serde_json::to_vec_pretty(snapshot).map_err(|e| AppError::json("gerar o backup", e))?;

        let mut arquivo = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&caminho)
            .map_err(|e| {
                if e.kind() == ErrorKind::AlreadyExists {
                    AppError::Invalido(format!(
                        "O backup \"{}\" já existe e não será sobrescrito.",
                        snapshot.id
                    ))
                } else {
                    AppError::io("gravar o backup", e)
                }
            })?;
        arquivo
            .write_all(&json)
            .and_then(|()| arquivo.sync_all())
            .map_err(|e| AppError::io("gravar o backup", e))
    }

    /// Captura o estado atual dos serviços e grava um novo snapshot.
    pub fn criar(
        &self,
        backend: &dyn ServiceBackend,
        tipo: SnapshotKind,
        motivo: &str,
    ) -> AppResult<SnapshotInfo> {
        let servicos = backend.list()?;
        if servicos.is_empty() {
            return Err(AppError::Outro(
                "Nenhum serviço foi lido do Windows; o backup não foi criado.".into(),
            ));
        }
        let id = match tipo {
            SnapshotKind::Original => ID_ORIGINAL.to_string(),
            _ => self.novo_id(),
        };
        let snapshot = Snapshot::capturar(id, tipo, motivo, &servicos);
        self.gravar_novo(&snapshot)?;
        if tipo == SnapshotKind::Automatico {
            // Limpeza de backups automáticos antigos é melhor-esforço.
            let _ = self.podar_automaticos(MAX_AUTOMATICOS);
        }
        Ok(snapshot.info())
    }

    /// Cria o snapshot original se ainda não existir. Devolve `true` se criou agora.
    pub fn garantir_original(&self, backend: &dyn ServiceBackend) -> AppResult<bool> {
        if self.caminho(ID_ORIGINAL)?.exists() {
            return Ok(false);
        }
        self.criar(
            backend,
            SnapshotKind::Original,
            "Estado original (primeira execução do WinTune)",
        )?;
        Ok(true)
    }

    pub fn carregar(&self, id: &str) -> AppResult<Snapshot> {
        let caminho = self.caminho(id)?;
        let conteudo = fs::read(&caminho).map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                AppError::Invalido(format!("Backup \"{id}\" não encontrado."))
            } else {
                AppError::io("ler o backup", e)
            }
        })?;
        serde_json::from_slice(&conteudo).map_err(|e| AppError::json("ler o backup", e))
    }

    /// Snapshot original, se já existir.
    pub fn original(&self) -> AppResult<Option<Snapshot>> {
        if !self.caminho(ID_ORIGINAL)?.exists() {
            return Ok(None);
        }
        self.carregar(ID_ORIGINAL).map(Some)
    }

    /// Lista os snapshots: o original primeiro, depois do mais novo para o mais antigo.
    /// Arquivos corrompidos são ignorados.
    pub fn listar(&self) -> AppResult<Vec<SnapshotInfo>> {
        let entradas = match fs::read_dir(&self.dir) {
            Ok(entradas) => entradas,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(AppError::io("listar os backups", e)),
        };

        let mut infos: Vec<SnapshotInfo> = entradas
            .filter_map(Result::ok)
            .map(|entrada| entrada.path())
            .filter(|caminho| caminho.extension().is_some_and(|ext| ext == "json"))
            .filter_map(|caminho| {
                let id = caminho.file_stem()?.to_str()?.to_string();
                let snapshot = self.carregar(&id).ok()?;
                // O nome do arquivo é a fonte da verdade para o id.
                Some(SnapshotInfo {
                    id,
                    ..snapshot.info()
                })
            })
            .collect();

        infos.sort_by(|a, b| {
            let a_original = a.tipo == SnapshotKind::Original;
            let b_original = b.tipo == SnapshotKind::Original;
            b_original
                .cmp(&a_original)
                .then_with(|| b.criado_em.cmp(&a.criado_em))
                .then_with(|| b.id.cmp(&a.id))
        });
        Ok(infos)
    }

    /// Exclui um snapshot. O original nunca pode ser excluído.
    pub fn excluir(&self, id: &str) -> AppResult<()> {
        let caminho = self.caminho(id)?;
        if id.eq_ignore_ascii_case(ID_ORIGINAL) {
            return Err(AppError::Invalido(
                "O backup original não pode ser excluído.".into(),
            ));
        }
        // Proteção extra: um arquivo marcado como original também não é excluído.
        if self.carregar(id)?.tipo == SnapshotKind::Original {
            return Err(AppError::Invalido(
                "O backup original não pode ser excluído.".into(),
            ));
        }
        fs::remove_file(&caminho).map_err(|e| AppError::io("excluir o backup", e))
    }

    /// Mantém apenas os `manter` snapshots automáticos mais recentes.
    pub fn podar_automaticos(&self, manter: usize) -> AppResult<usize> {
        let automaticos: Vec<_> = self
            .listar()?
            .into_iter()
            .filter(|s| s.tipo == SnapshotKind::Automatico)
            .collect();
        let mut removidos = 0;
        for antigo in automaticos.iter().skip(manter) {
            if self.excluir(&antigo.id).is_ok() {
                removidos += 1;
            }
        }
        Ok(removidos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scm::ServiceStatus;
    use crate::scm::mock::MockBackend;

    fn mock() -> MockBackend {
        let m = MockBackend::new();
        let m = MockBackend::com(m, "DiagTrack", StartType::Automatic, true);
        let m = MockBackend::com(m, "WSearch", StartType::AutomaticDelayed, true);
        MockBackend::com(m, "Fax", StartType::Manual, false)
    }

    fn store() -> (tempfile::TempDir, SnapshotStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SnapshotStore::new(dir.path().join("snapshots"));
        (dir, store)
    }

    #[test]
    fn snapshot_serializa_e_desserializa_sem_perdas() {
        let servicos = mock().list().unwrap();
        let snapshot = Snapshot::capturar(
            "20260101-120000-000".into(),
            SnapshotKind::Manual,
            "teste",
            &servicos,
        );

        let json = serde_json::to_string_pretty(&snapshot).unwrap();
        let volta: Snapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(volta, snapshot);

        // O formato em disco é estável: campos e valores em snake_case.
        let valor: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(valor["versao"], 1);
        assert_eq!(valor["tipo"], "manual");
        let wsearch = valor["servicos"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["name"] == "WSearch")
            .unwrap();
        assert_eq!(wsearch["start_type"], "automatic_delayed");
        assert_eq!(wsearch["rodando"], true);
    }

    #[test]
    fn le_snapshot_gravado_em_formato_conhecido() {
        let json = r#"{
            "versao": 1,
            "id": "original",
            "tipo": "original",
            "criado_em": "2026-03-04T10:20:30-03:00",
            "motivo": "Estado original",
            "servicos": [
                {"name": "Spooler", "display_name": "Spooler de Impressão",
                 "start_type": "automatic", "rodando": true},
                {"name": "Fax", "display_name": "Fax", "start_type": "disabled", "rodando": false}
            ]
        }"#;
        let snapshot: Snapshot = serde_json::from_str(json).unwrap();
        assert_eq!(snapshot.tipo, SnapshotKind::Original);
        assert_eq!(snapshot.servicos.len(), 2);
        assert_eq!(
            snapshot.servico("spooler").map(|s| s.start_type),
            Some(StartType::Automatic)
        );
        assert_eq!(snapshot.info().total_servicos, 2);
    }

    #[test]
    fn criar_e_carregar_do_disco() {
        let (_dir, store) = store();
        let backend = mock();
        let info = store
            .criar(&backend, SnapshotKind::Manual, "backup manual")
            .unwrap();
        assert_eq!(info.total_servicos, 3);

        let carregado = store.carregar(&info.id).unwrap();
        assert_eq!(carregado.info(), info);
        assert!(carregado.servico("DiagTrack").unwrap().rodando);
        assert!(!carregado.servico("Fax").unwrap().rodando);
    }

    #[test]
    fn original_e_criado_uma_vez_e_nunca_sobrescrito() {
        let (_dir, store) = store();
        let backend = mock();
        assert!(store.original().unwrap().is_none());
        assert!(store.garantir_original(&backend).unwrap());

        // O estado muda depois da primeira execução...
        backend
            .set_start_type("DiagTrack", StartType::Disabled)
            .unwrap();
        // ...mas o original permanece intacto.
        assert!(!store.garantir_original(&backend).unwrap());
        let erro = store
            .criar(&backend, SnapshotKind::Original, "de novo")
            .unwrap_err();
        assert!(erro.to_string().contains("não será sobrescrito"), "{erro}");

        let original = store.original().unwrap().unwrap();
        assert_eq!(original.tipo, SnapshotKind::Original);
        assert_eq!(
            original.servico("DiagTrack").unwrap().start_type,
            StartType::Automatic
        );
    }

    #[test]
    fn original_nao_pode_ser_excluido() {
        let (_dir, store) = store();
        let backend = mock();
        store.garantir_original(&backend).unwrap();
        assert!(store.excluir(ID_ORIGINAL).is_err());
        assert!(store.excluir("ORIGINAL").is_err());
        assert!(store.original().unwrap().is_some());

        let manual = store.criar(&backend, SnapshotKind::Manual, "x").unwrap();
        store.excluir(&manual.id).unwrap();
        assert!(store.carregar(&manual.id).is_err());
    }

    #[test]
    fn listar_coloca_original_primeiro_e_ignora_corrompidos() {
        let (_dir, store) = store();
        let backend = mock();
        let a = store.criar(&backend, SnapshotKind::Manual, "a").unwrap();
        let b = store
            .criar(&backend, SnapshotKind::Automatico, "b")
            .unwrap();
        store.garantir_original(&backend).unwrap();
        fs::write(store.dir().join("quebrado.json"), "{ não é json").unwrap();
        fs::write(store.dir().join("anotacao.txt"), "ignorar").unwrap();

        let lista = store.listar().unwrap();
        let ids: Vec<_> = lista.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec![ID_ORIGINAL, b.id.as_str(), a.id.as_str()]);
    }

    #[test]
    fn listar_sem_pasta_devolve_vazio() {
        let (_dir, store) = store();
        assert!(store.listar().unwrap().is_empty());
    }

    #[test]
    fn rejeita_ids_que_escapam_da_pasta() {
        let (_dir, store) = store();
        for id in [
            "",
            "../segredo",
            "a/b",
            "a\\b",
            "c:evil",
            "a.b",
            "nome com espaço",
        ] {
            assert!(store.carregar(id).is_err(), "aceitou id {id:?}");
            assert!(store.excluir(id).is_err(), "aceitou id {id:?}");
        }
    }

    #[test]
    fn ids_gerados_sao_unicos() {
        let (_dir, store) = store();
        let backend = mock();
        let mut ids = std::collections::HashSet::new();
        for _ in 0..5 {
            let info = store.criar(&backend, SnapshotKind::Manual, "x").unwrap();
            assert!(ids.insert(info.id));
        }
    }

    #[test]
    fn poda_mantem_apenas_os_automaticos_mais_recentes() {
        let (_dir, store) = store();
        let backend = mock();
        store.garantir_original(&backend).unwrap();
        let manual = store.criar(&backend, SnapshotKind::Manual, "m").unwrap();
        let autos: Vec<_> = (0..4)
            .map(|i| {
                store
                    .criar(&backend, SnapshotKind::Automatico, &format!("auto {i}"))
                    .unwrap()
            })
            .collect();

        assert_eq!(store.podar_automaticos(2).unwrap(), 2);
        let restantes: Vec<_> = store.listar().unwrap().into_iter().map(|s| s.id).collect();
        assert!(restantes.contains(&ID_ORIGINAL.to_string()));
        assert!(restantes.contains(&manual.id));
        assert!(restantes.contains(&autos[3].id));
        assert!(restantes.contains(&autos[2].id));
        assert!(!restantes.contains(&autos[0].id));
        assert!(!restantes.contains(&autos[1].id));
    }

    #[test]
    fn nao_cria_snapshot_vazio() {
        let (_dir, store) = store();
        let vazio = MockBackend::new();
        assert!(store.criar(&vazio, SnapshotKind::Manual, "x").is_err());
        assert!(store.garantir_original(&vazio).is_err());
        assert!(store.original().unwrap().is_none());
    }

    #[test]
    fn plano_de_restauracao_so_inclui_o_que_mudou() {
        let backend = mock();
        let snapshot = Snapshot::capturar(
            "s".into(),
            SnapshotKind::Manual,
            "antes",
            &backend.list().unwrap(),
        );

        backend
            .set_start_type("DiagTrack", StartType::Disabled)
            .unwrap();
        backend.stop("DiagTrack").unwrap();
        backend.set_start_type("Fax", StartType::Disabled).unwrap();

        let plano = snapshot.plano_de_restauracao(&backend.list().unwrap());
        assert_eq!(plano.len(), 2);

        let diag = plano.iter().find(|a| a.name == "DiagTrack").unwrap();
        assert_eq!(diag.start_type, StartType::Automatic);
        assert!(diag.garantir_rodando);

        let fax = plano.iter().find(|a| a.name == "Fax").unwrap();
        assert_eq!(fax.start_type, StartType::Manual);
        assert!(!fax.garantir_rodando);
    }

    #[test]
    fn plano_ignora_servicos_ausentes_e_tipos_desconhecidos() {
        let mut backend = MockBackend::com(MockBackend::new(), "A", StartType::Disabled, false);
        backend.inserir(ServiceEntry {
            name: "B".into(),
            display_name: "B".into(),
            status: ServiceStatus::Stopped,
            start_type: StartType::Disabled,
            per_user: false,
            acesso_negado: false,
        });
        let atuais = backend.list().unwrap();

        let snapshot = Snapshot {
            versao: 1,
            id: "s".into(),
            tipo: SnapshotKind::Manual,
            criado_em: Local::now(),
            motivo: String::new(),
            servicos: vec![
                SnapshotService {
                    name: "Removido".into(),
                    display_name: "Removido".into(),
                    start_type: StartType::Automatic,
                    rodando: true,
                },
                SnapshotService {
                    name: "B".into(),
                    display_name: "B".into(),
                    start_type: StartType::Unknown,
                    rodando: false,
                },
                SnapshotService {
                    name: "a".into(),
                    display_name: "A".into(),
                    start_type: StartType::Manual,
                    rodando: false,
                },
            ],
        };

        let plano = snapshot.plano_de_restauracao(&atuais);
        assert_eq!(plano.len(), 1);
        assert_eq!(plano[0].name, "A");
        assert_eq!(plano[0].start_type, StartType::Manual);
    }
}
