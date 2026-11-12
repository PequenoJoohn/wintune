//! Perfis: conjuntos de alterações pré-definidos (Leve, Gamer, Máximo) e personalizados.

use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::catalog::{Catalog, Risco};
use crate::error::{AppError, AppResult};
use crate::guard;
use crate::ops::{self, Alteracao, Situacao};
use crate::scm::{ServiceEntry, StartType};
use crate::snapshot::Snapshot;

pub const ID_LEVE: &str = "leve";
pub const ID_GAMER: &str = "gamer";
pub const ID_MAXIMO: &str = "maximo";
const PREFIXO_PERSONALIZADO: &str = "custom-";
const MAX_NOME: usize = 40;
const MAX_PERSONALIZADOS: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemPerfil {
    pub name: String,
    pub start_type: StartType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Perfil {
    pub id: String,
    pub nome: String,
    pub descricao: String,
    /// Perfil pré-definido (não pode ser excluído).
    #[serde(default)]
    pub embutido: bool,
    /// Exige confirmação extra no front antes de aplicar.
    #[serde(default)]
    pub confirmacao_extra: bool,
    pub itens: Vec<ItemPerfil>,
}

/// Linha da prévia (diff) de um perfil.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MudancaPrevista {
    pub name: String,
    pub display_name: String,
    pub de: Option<StartType>,
    pub para: StartType,
    pub situacao: Situacao,
    pub motivo: Option<String>,
    pub rodando: bool,
    pub risco: Option<Risco>,
    pub categoria: Option<String>,
}

impl Perfil {
    pub fn alteracoes(&self) -> Vec<Alteracao> {
        self.itens
            .iter()
            .map(|item| Alteracao {
                name: item.name.clone(),
                start_type: item.start_type,
                garantir_rodando: false,
            })
            .collect()
    }

    /// Prévia do que o perfil faria no estado atual, sem alterar nada.
    pub fn previa(&self, atuais: &[ServiceEntry], catalogo: &Catalog) -> Vec<MudancaPrevista> {
        ops::planejar(&self.alteracoes(), atuais)
            .into_iter()
            .map(|p| {
                let entrada = catalogo.get(&p.alteracao.name);
                MudancaPrevista {
                    display_name: p
                        .atual
                        .as_ref()
                        .map(|a| a.display_name.clone())
                        .unwrap_or_else(|| p.alteracao.name.clone()),
                    de: p.atual.as_ref().map(|a| a.start_type),
                    rodando: p.atual.as_ref().is_some_and(|a| a.status.is_ativo()),
                    para: p.alteracao.start_type,
                    situacao: p.situacao,
                    motivo: p.motivo,
                    risco: entrada.map(|e| e.risco),
                    categoria: entrada.map(|e| e.categoria.clone()),
                    name: p.alteracao.name,
                }
            })
            .collect()
    }
}

fn itens_do_catalogo(
    catalogo: &Catalog,
    filtro: impl Fn(&crate::catalog::CatalogEntry) -> bool,
) -> Vec<ItemPerfil> {
    catalogo
        .entradas()
        .iter()
        .filter(|e| filtro(e))
        .map(|e| ItemPerfil {
            name: e.name.clone(),
            start_type: e.recomendado,
        })
        .collect()
}

/// Perfis pré-definidos, derivados do catálogo. Serviços de risco "avançado"
/// nunca entram em nenhum perfil.
pub fn embutidos(catalogo: &Catalog) -> Vec<Perfil> {
    const XBOX: &str = "Xbox/Jogos";
    vec![
        Perfil {
            id: ID_LEVE.into(),
            nome: "Leve".into(),
            descricao: "Desativa apenas serviços de risco \"seguro\": telemetria, Xbox, fax e \
                        recursos que quase ninguém usa. Indicado para qualquer PC."
                .into(),
            embutido: true,
            confirmacao_extra: false,
            itens: itens_do_catalogo(catalogo, |e| e.risco == Risco::Seguro),
        },
        Perfil {
            id: ID_GAMER.into(),
            nome: "Gamer".into(),
            descricao: "Tudo do perfil Leve, mais indexação de busca e impressão. Mantém todos \
                        os serviços do Xbox para Game Pass e multiplayer."
                .into(),
            embutido: true,
            confirmacao_extra: false,
            itens: itens_do_catalogo(catalogo, |e| {
                e.categoria != XBOX
                    && (e.risco == Risco::Seguro
                        || (e.risco == Risco::Moderado
                            && matches!(e.categoria.as_str(), "Indexação/Busca" | "Impressão")))
            }),
        },
        Perfil {
            id: ID_MAXIMO.into(),
            nome: "Máximo".into(),
            descricao: "Desativa tudo do catálogo até o risco \"moderado\": inclui impressão, \
                        Bluetooth, busca, localização e Hyper-V. Leia a prévia com atenção."
                .into(),
            embutido: true,
            confirmacao_extra: true,
            itens: itens_do_catalogo(catalogo, |e| e.risco <= Risco::Moderado),
        },
    ]
}

/// Itens que representam "a seleção atual": todo serviço não protegido cujo tipo de
/// inicialização hoje difere do snapshot original.
pub fn itens_da_selecao_atual(original: &Snapshot, atuais: &[ServiceEntry]) -> Vec<ItemPerfil> {
    atuais
        .iter()
        .filter(|atual| !atual.per_user && atual.start_type != StartType::Unknown)
        .filter(|atual| guard::validar_start_type(&atual.name, atual.start_type).is_ok())
        .filter(|atual| {
            original
                .servico(&atual.name)
                .is_some_and(|salvo| salvo.start_type != atual.start_type)
        })
        .map(|atual| ItemPerfil {
            name: atual.name.clone(),
            start_type: atual.start_type,
        })
        .collect()
}

/// Perfis personalizados, gravados em `%APPDATA%/WinTune/profiles.json`.
#[derive(Debug, Clone)]
pub struct ProfileStore {
    arquivo: PathBuf,
}

impl ProfileStore {
    pub fn new(arquivo: impl Into<PathBuf>) -> Self {
        Self {
            arquivo: arquivo.into(),
        }
    }

    pub fn personalizados(&self) -> AppResult<Vec<Perfil>> {
        let conteudo = match fs::read(&self.arquivo) {
            Ok(conteudo) => conteudo,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(AppError::io("ler os perfis personalizados", e)),
        };
        let mut perfis: Vec<Perfil> = serde_json::from_slice(&conteudo)
            .map_err(|e| AppError::json("ler os perfis personalizados", e))?;

        // O arquivo pode ter sido editado à mão: nunca confiar nele.
        perfis.retain(|p| p.id.starts_with(PREFIXO_PERSONALIZADO));
        for perfil in &mut perfis {
            perfil.embutido = false;
            perfil.confirmacao_extra = true;
            perfil
                .itens
                .retain(|item| guard::validar_start_type(&item.name, item.start_type).is_ok());
        }
        Ok(perfis)
    }

    fn gravar(&self, perfis: &[Perfil]) -> AppResult<()> {
        if let Some(pasta) = self.arquivo.parent() {
            fs::create_dir_all(pasta).map_err(|e| AppError::io("criar a pasta de dados", e))?;
        }
        let json = serde_json::to_vec_pretty(perfis)
            .map_err(|e| AppError::json("gravar os perfis personalizados", e))?;
        // Grava em arquivo temporário e renomeia, para nunca deixar o JSON pela metade.
        let temporario = self.arquivo.with_extension("json.tmp");
        fs::write(&temporario, json)
            .and_then(|()| fs::rename(&temporario, &self.arquivo))
            .map_err(|e| AppError::io("gravar os perfis personalizados", e))
    }

    /// Salva um novo perfil personalizado. Itens protegidos são rejeitados.
    pub fn salvar(&self, nome: &str, itens: Vec<ItemPerfil>) -> AppResult<Perfil> {
        let nome = nome.trim();
        if nome.is_empty() {
            return Err(AppError::Invalido("Dê um nome ao perfil.".into()));
        }
        if nome.chars().count() > MAX_NOME {
            return Err(AppError::Invalido(format!(
                "O nome do perfil deve ter no máximo {MAX_NOME} caracteres."
            )));
        }
        if itens.is_empty() {
            return Err(AppError::Invalido(
                "Nenhum serviço foi alterado em relação ao estado original; não há o que salvar."
                    .into(),
            ));
        }
        let mut vistos = HashSet::new();
        for item in &itens {
            guard::validar_start_type(&item.name, item.start_type)?;
            if !vistos.insert(item.name.to_lowercase()) {
                return Err(AppError::Invalido(format!(
                    "O serviço \"{}\" aparece mais de uma vez no perfil.",
                    item.name
                )));
            }
        }

        let mut perfis = self.personalizados()?;
        if perfis.len() >= MAX_PERSONALIZADOS {
            return Err(AppError::Invalido(format!(
                "Limite de {MAX_PERSONALIZADOS} perfis personalizados atingido. Exclua algum antes de salvar outro."
            )));
        }
        if perfis.iter().any(|p| p.nome.eq_ignore_ascii_case(nome)) {
            return Err(AppError::Invalido(format!(
                "Já existe um perfil chamado \"{nome}\"."
            )));
        }

        let agora = Local::now();
        let perfil = Perfil {
            id: format!(
                "{PREFIXO_PERSONALIZADO}{}",
                agora.format("%Y%m%d-%H%M%S-%3f")
            ),
            nome: nome.to_string(),
            descricao: format!(
                "Perfil personalizado salvo em {} com {} serviço(s) alterado(s).",
                agora.format("%d/%m/%Y %H:%M"),
                itens.len()
            ),
            embutido: false,
            confirmacao_extra: true,
            itens,
        };
        perfis.push(perfil.clone());
        self.gravar(&perfis)?;
        Ok(perfil)
    }

    pub fn excluir(&self, id: &str) -> AppResult<()> {
        if !id.starts_with(PREFIXO_PERSONALIZADO) {
            return Err(AppError::Invalido(
                "Perfis pré-definidos não podem ser excluídos.".into(),
            ));
        }
        let mut perfis = self.personalizados()?;
        let antes = perfis.len();
        perfis.retain(|p| p.id != id);
        if perfis.len() == antes {
            return Err(AppError::Invalido("Perfil não encontrado.".into()));
        }
        self.gravar(&perfis)
    }
}

/// Todos os perfis: pré-definidos primeiro, depois os personalizados.
pub fn todos(catalogo: &Catalog, store: &ProfileStore) -> AppResult<Vec<Perfil>> {
    let mut perfis = embutidos(catalogo);
    perfis.extend(store.personalizados()?);
    Ok(perfis)
}

pub fn buscar(catalogo: &Catalog, store: &ProfileStore, id: &str) -> AppResult<Perfil> {
    todos(catalogo, store)?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| AppError::Invalido(format!("Perfil \"{id}\" não encontrado.")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guard::Protecao;
    use crate::scm::ServiceBackend;
    use crate::scm::mock::MockBackend;
    use crate::snapshot::SnapshotKind;

    fn catalogo() -> &'static Catalog {
        Catalog::embutido().unwrap()
    }

    fn perfil(id: &str) -> Perfil {
        embutidos(catalogo())
            .into_iter()
            .find(|p| p.id == id)
            .unwrap()
    }

    fn nomes(p: &Perfil) -> HashSet<String> {
        p.itens.iter().map(|i| i.name.clone()).collect()
    }

    fn store() -> (tempfile::TempDir, ProfileStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ProfileStore::new(dir.path().join("WinTune").join("profiles.json"));
        (dir, store)
    }

    #[test]
    fn leve_tem_apenas_itens_seguros() {
        let leve = perfil(ID_LEVE);
        assert!(!leve.itens.is_empty());
        for item in &leve.itens {
            assert_eq!(
                catalogo().get(&item.name).unwrap().risco,
                Risco::Seguro,
                "{}",
                item.name
            );
        }
        assert!(nomes(&leve).contains("DiagTrack"));
        assert!(!nomes(&leve).contains("Spooler"));
    }

    #[test]
    fn gamer_inclui_busca_e_impressao_e_mantem_xbox() {
        let gamer = nomes(&perfil(ID_GAMER));
        for name in ["DiagTrack", "WSearch", "Spooler", "PrintNotify", "Fax"] {
            assert!(gamer.contains(name), "faltou {name}");
        }
        for name in [
            "XblAuthManager",
            "XblGameSave",
            "XboxNetApiSvc",
            "XboxGipSvc",
        ] {
            assert!(!gamer.contains(name), "Gamer não pode desativar {name}");
        }
        // Moderados fora de busca/impressão ficam de fora.
        assert!(!gamer.contains("bthserv"));
        assert!(!gamer.contains("SysMain"));

        // Gamer = Leve (sem Xbox) + extras.
        for name in nomes(&perfil(ID_LEVE)) {
            if catalogo().get(&name).unwrap().categoria != "Xbox/Jogos" {
                assert!(gamer.contains(&name), "Gamer deveria conter {name}");
            }
        }
    }

    #[test]
    fn maximo_vai_ate_moderado_e_pede_confirmacao_extra() {
        let maximo = perfil(ID_MAXIMO);
        assert!(maximo.confirmacao_extra);
        assert!(!perfil(ID_LEVE).confirmacao_extra);
        let itens = nomes(&maximo);
        for name in ["Spooler", "bthserv", "SysMain", "XblGameSave", "vmicvss"] {
            assert!(itens.contains(name), "faltou {name}");
        }
        assert!(nomes(&perfil(ID_LEVE)).is_subset(&itens));
        assert!(nomes(&perfil(ID_GAMER)).is_subset(&itens));
    }

    #[test]
    fn nenhum_perfil_embutido_toca_em_avancados_ou_protegidos() {
        for p in embutidos(catalogo()) {
            assert!(p.embutido);
            for item in &p.itens {
                let entrada = catalogo().get(&item.name).unwrap();
                assert_ne!(entrada.risco, Risco::Avancado, "{} em {}", item.name, p.id);
                assert_eq!(guard::classificar(&item.name), Protecao::Livre);
            }
            for name in ["TermService", "WbioSrvc", "TabletInputService"] {
                assert!(!nomes(&p).contains(name));
            }
        }
    }

    #[test]
    fn previa_mostra_o_diff_sem_alterar_nada() {
        let m = MockBackend::new();
        let m = MockBackend::com(m, "DiagTrack", StartType::Automatic, true);
        let backend = MockBackend::com(m, "Fax", StartType::Disabled, false);

        let previa = perfil(ID_LEVE).previa(&backend.list().unwrap(), catalogo());
        let buscar = |name: &str| previa.iter().find(|m| m.name == name).unwrap();

        let diag = buscar("DiagTrack");
        assert_eq!(diag.situacao, Situacao::Alterar);
        assert_eq!(diag.de, Some(StartType::Automatic));
        assert_eq!(diag.para, StartType::Disabled);
        assert!(diag.rodando);
        assert_eq!(diag.risco, Some(Risco::Seguro));

        assert_eq!(buscar("Fax").situacao, Situacao::SemMudanca);
        let ausente = buscar("RetailDemo");
        assert_eq!(ausente.situacao, Situacao::Ausente);
        assert_eq!(ausente.de, None);

        assert!(backend.chamadas().is_empty());
    }

    #[test]
    fn selecao_atual_contem_apenas_o_que_difere_do_original() {
        let m = MockBackend::new();
        let m = MockBackend::com(m, "DiagTrack", StartType::Automatic, true);
        let m = MockBackend::com(m, "Fax", StartType::Manual, false);
        let m = MockBackend::com(m, "Spooler", StartType::Automatic, true);
        let backend = MockBackend::com(m, "RpcSs", StartType::Automatic, true);
        let original = Snapshot::capturar(
            "original".into(),
            SnapshotKind::Original,
            "",
            &backend.list().unwrap(),
        );

        backend
            .set_start_type("DiagTrack", StartType::Disabled)
            .unwrap();
        backend
            .set_start_type("Spooler", StartType::Manual)
            .unwrap();
        // Mesmo que algo externo mude um serviço crítico, ele nunca entra no perfil.
        backend.set_start_type("RpcSs", StartType::Manual).unwrap();

        let mut itens = itens_da_selecao_atual(&original, &backend.list().unwrap());
        itens.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(
            itens,
            vec![
                ItemPerfil {
                    name: "DiagTrack".into(),
                    start_type: StartType::Disabled
                },
                ItemPerfil {
                    name: "Spooler".into(),
                    start_type: StartType::Manual
                },
            ]
        );
    }

    #[test]
    fn salva_lista_e_exclui_perfil_personalizado() {
        let (_dir, store) = store();
        assert!(store.personalizados().unwrap().is_empty());

        let itens = vec![ItemPerfil {
            name: "DiagTrack".into(),
            start_type: StartType::Disabled,
        }];
        let salvo = store.salvar("  Meu perfil  ", itens.clone()).unwrap();
        assert_eq!(salvo.nome, "Meu perfil");
        assert!(salvo.id.starts_with("custom-"));
        assert!(!salvo.embutido);

        let todos = todos(catalogo(), &store).unwrap();
        assert_eq!(todos.len(), 4);
        assert_eq!(todos[3], salvo);
        assert_eq!(buscar(catalogo(), &store, &salvo.id).unwrap(), salvo);

        assert!(store.salvar("meu PERFIL", itens).is_err(), "nome repetido");

        store.excluir(&salvo.id).unwrap();
        assert!(store.personalizados().unwrap().is_empty());
        assert!(store.excluir(&salvo.id).is_err());
    }

    #[test]
    fn rejeita_perfis_invalidos() {
        let (_dir, store) = store();
        let item = |name: &str, start_type| ItemPerfil {
            name: name.into(),
            start_type,
        };

        assert!(
            store
                .salvar("", vec![item("Fax", StartType::Disabled)])
                .is_err()
        );
        assert!(
            store
                .salvar(&"x".repeat(41), vec![item("Fax", StartType::Disabled)])
                .is_err()
        );
        assert!(store.salvar("Vazio", vec![]).is_err());
        assert!(matches!(
            store.salvar("Perigoso", vec![item("RpcSs", StartType::Disabled)]),
            Err(AppError::ServicoBloqueado(_))
        ));
        assert!(matches!(
            store.salvar("Update", vec![item("wuauserv", StartType::Disabled)]),
            Err(AppError::ServicoRestrito(_))
        ));
        assert!(
            store
                .salvar(
                    "Duplicado",
                    vec![
                        item("Fax", StartType::Disabled),
                        item("fax", StartType::Manual)
                    ]
                )
                .is_err()
        );
        assert!(store.personalizados().unwrap().is_empty());
    }

    #[test]
    fn perfis_embutidos_nao_podem_ser_excluidos() {
        let (_dir, store) = store();
        for id in [ID_LEVE, ID_GAMER, ID_MAXIMO] {
            assert!(store.excluir(id).is_err());
        }
        assert!(buscar(catalogo(), &store, "inexistente").is_err());
    }

    #[test]
    fn arquivo_editado_a_mao_nao_burla_o_guard() {
        let (_dir, store) = store();
        fs::create_dir_all(store.arquivo.parent().unwrap()).unwrap();
        fs::write(
            &store.arquivo,
            r#"[
                {"id":"custom-1","nome":"Malicioso","descricao":"","embutido":true,
                 "itens":[{"name":"RpcSs","start_type":"disabled"},
                          {"name":"wuauserv","start_type":"disabled"},
                          {"name":"Fax","start_type":"disabled"}]},
                {"id":"leve","nome":"Falso Leve","descricao":"","itens":[]}
            ]"#,
        )
        .unwrap();

        let perfis = store.personalizados().unwrap();
        assert_eq!(perfis.len(), 1, "id que imita perfil embutido é descartado");
        assert!(!perfis[0].embutido);
        assert!(perfis[0].confirmacao_extra);
        assert_eq!(
            perfis[0].itens,
            vec![ItemPerfil {
                name: "Fax".into(),
                start_type: StartType::Disabled
            }]
        );

        // O perfil "leve" verdadeiro continua sendo o embutido.
        assert_eq!(buscar(catalogo(), &store, ID_LEVE).unwrap().nome, "Leve");
    }
}
