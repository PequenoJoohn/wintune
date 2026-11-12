//! Lista de bloqueio: serviços que o WinTune NUNCA altera, mesmo que o front peça.
//!
//! Toda operação de escrita passa por [`validar_start_type`], [`validar_parada`] ou
//! [`validar_inicio`] antes de chegar ao SCM.

use crate::error::{AppError, AppResult};
use crate::scm::StartType;

/// Serviços críticos: nenhuma alteração é permitida (tipo de inicialização, parar ou iniciar).
///
/// A primeira parte é o mínimo exigido pelo projeto; a segunda são acréscimos conservadores
/// de serviços cuja desativação quebra login, rede, Microsoft Store, ativação ou atualizações.
pub const BLOQUEADOS: &[&str] = &[
    // --- mínimo obrigatório ---
    "RpcSs",
    "RpcEptMapper",
    "DcomLaunch",
    "Winmgmt",
    "CryptSvc",
    "BFE",
    "MpsSvc",
    "WinDefend",
    "SecurityHealthService",
    "EventLog",
    "PlugPlay",
    "Power",
    "ProfSvc",
    "LSM",
    "SamSs",
    "Schedule",
    "BrokerInfrastructure",
    "CoreMessagingRegistrar",
    "gpsvc",
    "Dhcp",
    "Dnscache",
    "nsi",
    "AudioSrv",
    "AudioEndpointBuilder",
    "Themes",
    "UserManager",
    "TrustedInstaller",
    // --- acréscimos conservadores ---
    "Appinfo",
    "AppXSvc",
    "ClipSVC",
    "DeviceInstall",
    "EventSystem",
    "KeyIso",
    "LanmanWorkstation",
    "msiserver",
    "netprofm",
    "NlaSvc",
    "SENS",
    "Sense",
    "sppsvc",
    "StateRepository",
    "StorSvc",
    "SystemEventsBroker",
    "TextInputManagementService",
    "TimeBrokerSvc",
    "TokenBroker",
    "UsoSvc",
    "VaultSvc",
    "WaaSMedicSvc",
    "Wcmsvc",
    "WdNisSvc",
    "WlanSvc",
    "wscsvc",
];

/// Serviços que podem mudar de tipo de inicialização, mas nunca para Desativado
/// e nunca são parados pelo app.
pub const RESTRITOS: &[&str] = &["wuauserv"];

/// Nível de proteção de um serviço.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protecao {
    /// Pode ser alterado normalmente.
    Livre,
    /// Crítico: nenhuma alteração.
    Bloqueado,
    /// Aceita Manual/Automático, nunca Desativado.
    Restrito,
    /// Serviço por usuário (sufixo `_xxxxx`): gerenciado pelo Windows.
    PorUsuario,
}

fn contem(lista: &[&str], name: &str) -> bool {
    lista.iter().any(|item| item.eq_ignore_ascii_case(name))
}

/// `true` para nomes de instância de serviço por usuário, como `CDPUserSvc_3f2a1`.
pub fn tem_sufixo_por_usuario(name: &str) -> bool {
    match name.rsplit_once('_') {
        Some((prefixo, sufixo)) => {
            !prefixo.is_empty()
                && (4..=8).contains(&sufixo.len())
                && sufixo.chars().all(|c| c.is_ascii_hexdigit())
        }
        None => false,
    }
}

/// Classifica um serviço apenas pelo nome.
pub fn classificar(name: &str) -> Protecao {
    let name = name.trim();
    if contem(BLOQUEADOS, name) {
        Protecao::Bloqueado
    } else if contem(RESTRITOS, name) {
        Protecao::Restrito
    } else if tem_sufixo_por_usuario(name) {
        Protecao::PorUsuario
    } else {
        Protecao::Livre
    }
}

/// `true` se o serviço está em qualquer lista de proteção por nome.
pub fn is_bloqueado(name: &str) -> bool {
    classificar(name) == Protecao::Bloqueado
}

/// Texto curto explicando a proteção, para exibir no front.
pub fn motivo(protecao: Protecao) -> Option<&'static str> {
    match protecao {
        Protecao::Livre => None,
        Protecao::Bloqueado => Some("Serviço essencial do Windows. O WinTune nunca o altera."),
        Protecao::Restrito => {
            Some("Serviço restrito: pode ficar em Manual ou Automático, mas nunca Desativado.")
        }
        Protecao::PorUsuario => {
            Some("Serviço por usuário, criado e gerenciado pelo próprio Windows.")
        }
    }
}

/// Valida uma mudança de tipo de inicialização.
pub fn validar_start_type(name: &str, alvo: StartType) -> AppResult<()> {
    if alvo == StartType::Unknown {
        return Err(AppError::Invalido(
            "Tipo de inicialização inválido.".to_string(),
        ));
    }
    match classificar(name) {
        Protecao::Livre => Ok(()),
        Protecao::Bloqueado => Err(AppError::ServicoBloqueado(name.to_string())),
        Protecao::PorUsuario => Err(AppError::ServicoPorUsuario(name.to_string())),
        Protecao::Restrito if alvo == StartType::Disabled => {
            Err(AppError::ServicoRestrito(name.to_string()))
        }
        Protecao::Restrito => Ok(()),
    }
}

/// Valida a parada de um serviço.
pub fn validar_parada(name: &str) -> AppResult<()> {
    match classificar(name) {
        Protecao::Livre => Ok(()),
        Protecao::Bloqueado => Err(AppError::ServicoBloqueado(name.to_string())),
        Protecao::Restrito => Err(AppError::ServicoRestrito(name.to_string())),
        Protecao::PorUsuario => Err(AppError::ServicoPorUsuario(name.to_string())),
    }
}

/// Valida o início de um serviço. Serviços restritos podem ser iniciados.
pub fn validar_inicio(name: &str) -> AppResult<()> {
    match classificar(name) {
        Protecao::Livre | Protecao::Restrito => Ok(()),
        Protecao::Bloqueado => Err(AppError::ServicoBloqueado(name.to_string())),
        Protecao::PorUsuario => Err(AppError::ServicoPorUsuario(name.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lista mínima exigida pela especificação do projeto.
    const MINIMO_OBRIGATORIO: &[&str] = &[
        "RpcSs",
        "RpcEptMapper",
        "DcomLaunch",
        "Winmgmt",
        "CryptSvc",
        "BFE",
        "MpsSvc",
        "WinDefend",
        "SecurityHealthService",
        "EventLog",
        "PlugPlay",
        "Power",
        "ProfSvc",
        "LSM",
        "SamSs",
        "Schedule",
        "BrokerInfrastructure",
        "CoreMessagingRegistrar",
        "gpsvc",
        "Dhcp",
        "Dnscache",
        "nsi",
        "AudioSrv",
        "AudioEndpointBuilder",
        "Themes",
        "UserManager",
        "TrustedInstaller",
    ];

    const TODOS_OS_TIPOS: [StartType; 4] = [
        StartType::Automatic,
        StartType::AutomaticDelayed,
        StartType::Manual,
        StartType::Disabled,
    ];

    #[test]
    fn lista_minima_esta_totalmente_bloqueada() {
        for name in MINIMO_OBRIGATORIO {
            assert_eq!(classificar(name), Protecao::Bloqueado, "{name}");
            for tipo in TODOS_OS_TIPOS {
                assert!(
                    matches!(
                        validar_start_type(name, tipo),
                        Err(AppError::ServicoBloqueado(_))
                    ),
                    "{name} aceitou {tipo:?}"
                );
            }
            assert!(validar_parada(name).is_err(), "{name} aceitou parada");
            assert!(validar_inicio(name).is_err(), "{name} aceitou início");
        }
    }

    #[test]
    fn bloqueio_ignora_maiusculas_e_espacos() {
        assert!(is_bloqueado("rpcss"));
        assert!(is_bloqueado("RPCSS"));
        assert!(is_bloqueado("  WinDefend "));
        assert!(validar_start_type("eventlog", StartType::Manual).is_err());
    }

    #[test]
    fn wuauserv_aceita_manual_mas_nunca_desativado() {
        assert_eq!(classificar("wuauserv"), Protecao::Restrito);
        assert!(validar_start_type("wuauserv", StartType::Manual).is_ok());
        assert!(validar_start_type("WUAUSERV", StartType::Automatic).is_ok());
        assert!(matches!(
            validar_start_type("wuauserv", StartType::Disabled),
            Err(AppError::ServicoRestrito(_))
        ));
        assert!(matches!(
            validar_parada("wuauserv"),
            Err(AppError::ServicoRestrito(_))
        ));
        assert!(validar_inicio("wuauserv").is_ok());
    }

    #[test]
    fn servicos_por_usuario_sao_protegidos() {
        for name in [
            "CDPUserSvc_3f2a1",
            "OneSyncSvc_1a2b3c",
            "WpnUserService_ab12",
        ] {
            assert_eq!(classificar(name), Protecao::PorUsuario, "{name}");
            assert!(matches!(
                validar_start_type(name, StartType::Disabled),
                Err(AppError::ServicoPorUsuario(_))
            ));
            assert!(validar_parada(name).is_err());
        }
    }

    #[test]
    fn sufixo_por_usuario_nao_gera_falso_positivo_obvio() {
        for name in [
            "DiagTrack",
            "WSearch",
            "_abcd",
            "Servico_xyz1",
            "Servico_12",
            "Servico_123456789",
            "diagnosticshub.standardcollector.service",
        ] {
            assert!(!tem_sufixo_por_usuario(name), "{name}");
        }
    }

    #[test]
    fn servicos_comuns_ficam_livres() {
        for name in ["DiagTrack", "WSearch", "Spooler", "Fax", "XblGameSave"] {
            assert_eq!(classificar(name), Protecao::Livre, "{name}");
            assert!(validar_start_type(name, StartType::Disabled).is_ok());
            assert!(validar_parada(name).is_ok());
            assert!(validar_inicio(name).is_ok());
        }
    }

    #[test]
    fn start_type_desconhecido_e_sempre_rejeitado() {
        assert!(matches!(
            validar_start_type("DiagTrack", StartType::Unknown),
            Err(AppError::Invalido(_))
        ));
    }

    #[test]
    fn listas_nao_tem_duplicatas_nem_sobreposicao() {
        let mut vistos = std::collections::HashSet::new();
        for name in BLOQUEADOS.iter().chain(RESTRITOS) {
            assert!(vistos.insert(name.to_lowercase()), "duplicado: {name}");
        }
    }

    #[test]
    fn todo_nivel_protegido_tem_motivo() {
        assert!(motivo(Protecao::Livre).is_none());
        for p in [
            Protecao::Bloqueado,
            Protecao::Restrito,
            Protecao::PorUsuario,
        ] {
            assert!(motivo(p).is_some());
        }
    }
}
