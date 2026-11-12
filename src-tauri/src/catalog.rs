//! Catálogo curado de serviços (`catalog.json`, embutido no binário).

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::guard::{self, Protecao};
use crate::scm::StartType;

const CATALOGO_JSON: &str = include_str!("../catalog.json");

/// Categorias aceitas no catálogo.
pub const CATEGORIAS: &[&str] = &[
    "Telemetria",
    "Xbox/Jogos",
    "Impressão",
    "Indexação/Busca",
    "Rede remota",
    "Bluetooth",
    "Hyper-V",
    "Mapas/Localização",
    "Fax",
    "Diversos",
];

/// Nível de risco de desativar um serviço.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risco {
    Seguro,
    Moderado,
    Avancado,
}

/// Uma entrada do catálogo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    pub name: String,
    pub categoria: String,
    pub risco: Risco,
    /// O que o serviço faz e o que para de funcionar se for desativado (PT-BR).
    pub descricao: String,
    /// Tipo de inicialização sugerido pelo WinTune.
    pub recomendado: StartType,
}

/// Catálogo carregado e indexado por nome (sem diferenciar maiúsculas).
#[derive(Debug)]
pub struct Catalog {
    entradas: Vec<CatalogEntry>,
    indice: HashMap<String, usize>,
}

impl Catalog {
    /// Lê e valida um catálogo a partir de JSON.
    pub fn from_json(json: &str) -> AppResult<Self> {
        let entradas: Vec<CatalogEntry> =
            serde_json::from_str(json).map_err(|e| AppError::json("ler o catálogo", e))?;
        Self::from_entries(entradas)
    }

    fn from_entries(entradas: Vec<CatalogEntry>) -> AppResult<Self> {
        let mut indice = HashMap::with_capacity(entradas.len());
        let mut vistos = HashSet::new();

        for (posicao, entrada) in entradas.iter().enumerate() {
            let invalido = |motivo: &str| {
                AppError::Invalido(format!(
                    "Catálogo inválido: serviço \"{}\" {motivo}.",
                    entrada.name
                ))
            };

            if entrada.name.trim().is_empty() || entrada.name != entrada.name.trim() {
                return Err(invalido("tem nome vazio ou com espaços"));
            }
            if !vistos.insert(entrada.name.to_lowercase()) {
                return Err(invalido("está duplicado"));
            }
            if !CATEGORIAS.contains(&entrada.categoria.as_str()) {
                return Err(invalido("usa uma categoria desconhecida"));
            }
            if entrada.descricao.trim().len() < 20 {
                return Err(invalido("não tem descrição suficiente"));
            }
            if !matches!(entrada.recomendado, StartType::Manual | StartType::Disabled) {
                return Err(invalido("deve recomendar Manual ou Desativado"));
            }
            // Regra de segurança: o catálogo nunca pode sugerir mexer em serviço protegido.
            if guard::classificar(&entrada.name) != Protecao::Livre {
                return Err(invalido("está na lista de bloqueio"));
            }
            indice.insert(entrada.name.to_lowercase(), posicao);
        }

        Ok(Self { entradas, indice })
    }

    /// Catálogo embutido no binário (carregado e validado uma única vez).
    pub fn embutido() -> AppResult<&'static Catalog> {
        static CATALOGO: OnceLock<Result<Catalog, String>> = OnceLock::new();
        CATALOGO
            .get_or_init(|| Catalog::from_json(CATALOGO_JSON).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|mensagem| AppError::Outro(mensagem.clone()))
    }

    pub fn entradas(&self) -> &[CatalogEntry] {
        &self.entradas
    }

    pub fn get(&self, name: &str) -> Option<&CatalogEntry> {
        self.indice
            .get(&name.to_lowercase())
            .and_then(|posicao| self.entradas.get(*posicao))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serviços que a especificação exige no catálogo.
    const OBRIGATORIOS: &[&str] = &[
        "DiagTrack",
        "dmwappushservice",
        "SysMain",
        "WSearch",
        "Fax",
        "Spooler",
        "PrintNotify",
        "XblAuthManager",
        "XblGameSave",
        "XboxNetApiSvc",
        "XboxGipSvc",
        "RemoteRegistry",
        "RemoteAccess",
        "MapsBroker",
        "lfsvc",
        "RetailDemo",
        "WbioSrvc",
        "bthserv",
        "TabletInputService",
        "WerSvc",
        "wisvc",
        "PhoneSvc",
        "SEMgrSvc",
        "WMPNetworkSvc",
        "icssvc",
        "TermService",
        "SessionEnv",
        "UmRdpService",
        "vmicguestinterface",
        "vmicheartbeat",
        "vmickvpexchange",
        "vmicrdv",
        "vmicshutdown",
        "vmictimesync",
        "vmicvmsession",
        "vmicvss",
    ];

    fn catalogo() -> &'static Catalog {
        Catalog::embutido().expect("catalog.json embutido deve ser válido")
    }

    #[test]
    fn json_embutido_e_valido() {
        assert!(catalogo().entradas().len() >= OBRIGATORIOS.len());
    }

    #[test]
    fn contem_todos_os_servicos_obrigatorios() {
        for name in OBRIGATORIOS {
            assert!(catalogo().get(name).is_some(), "faltou no catálogo: {name}");
        }
    }

    #[test]
    fn nenhum_servico_do_catalogo_esta_na_lista_de_bloqueio() {
        for entrada in catalogo().entradas() {
            assert_eq!(
                guard::classificar(&entrada.name),
                Protecao::Livre,
                "{} está protegido e não pode constar no catálogo",
                entrada.name
            );
            assert!(guard::validar_start_type(&entrada.name, entrada.recomendado).is_ok());
            assert!(guard::validar_parada(&entrada.name).is_ok());
        }
    }

    #[test]
    fn entradas_tem_campos_coerentes() {
        let mut nomes = HashSet::new();
        for entrada in catalogo().entradas() {
            assert!(
                nomes.insert(entrada.name.to_lowercase()),
                "duplicado: {}",
                entrada.name
            );
            assert!(
                CATEGORIAS.contains(&entrada.categoria.as_str()),
                "categoria inválida em {}: {}",
                entrada.name,
                entrada.categoria
            );
            assert!(
                entrada.descricao.len() >= 40,
                "descrição curta em {}",
                entrada.name
            );
            assert!(matches!(
                entrada.recomendado,
                StartType::Manual | StartType::Disabled
            ));
        }
    }

    #[test]
    fn toda_categoria_tem_ao_menos_um_servico() {
        for categoria in CATEGORIAS {
            assert!(
                catalogo()
                    .entradas()
                    .iter()
                    .any(|e| e.categoria == *categoria),
                "categoria sem serviços: {categoria}"
            );
        }
    }

    #[test]
    fn busca_ignora_maiusculas() {
        let entrada = catalogo().get("DIAGTRACK").expect("DiagTrack");
        assert_eq!(entrada.name, "DiagTrack");
        assert_eq!(entrada.risco, Risco::Seguro);
        assert!(catalogo().get("NaoExiste").is_none());
    }

    #[test]
    fn servicos_de_acesso_remoto_e_login_sao_avancados() {
        // Desativá-los pode trancar o usuário fora da máquina: nunca entram em perfis.
        for name in ["TermService", "SessionEnv", "UmRdpService", "WbioSrvc"] {
            assert_eq!(
                catalogo().get(name).map(|e| e.risco),
                Some(Risco::Avancado),
                "{name}"
            );
        }
    }

    fn entrada_json(name: &str, categoria: &str, recomendado: &str) -> String {
        format!(
            r#"[{{"name":"{name}","categoria":"{categoria}","risco":"seguro",
               "descricao":"Descrição longa o bastante para passar na validação.",
               "recomendado":"{recomendado}"}}]"#
        )
    }

    #[test]
    fn rejeita_catalogo_com_servico_bloqueado() {
        for name in ["RpcSs", "wuauserv", "windefend", "CDPUserSvc_3f2a1"] {
            let erro = Catalog::from_json(&entrada_json(name, "Diversos", "disabled"))
                .expect_err("deveria rejeitar");
            assert!(erro.to_string().contains("lista de bloqueio"), "{erro}");
        }
    }

    #[test]
    fn rejeita_catalogo_malformado() {
        assert!(Catalog::from_json("não é json").is_err());
        assert!(Catalog::from_json(&entrada_json("Foo", "Inexistente", "disabled")).is_err());
        assert!(Catalog::from_json(&entrada_json("Foo", "Diversos", "automatic")).is_err());
        assert!(Catalog::from_json(&entrada_json("Foo", "Diversos", "qualquer")).is_err());

        let duplicado = format!(
            "[{0},{0}]",
            entrada_json("Foo", "Diversos", "disabled")
                .trim_start_matches('[')
                .trim_end_matches(']')
        );
        assert!(Catalog::from_json(&duplicado).is_err());
        assert!(Catalog::from_json(&entrada_json("Foo", "Diversos", "disabled")).is_ok());
    }
}
