//! Wrapper sobre o Service Control Manager (SCM) do Windows.
//!
//! Toda a lógica do app fala com a trait [`ServiceBackend`]; a implementação real
//! ([`WindowsScm`]) fica isolada aqui e os testes usam [`mock::MockBackend`].
//!
//! Decisão de segurança: o tipo de inicialização é alterado com `ChangeServiceConfigW`
//! passando `SERVICE_NO_CHANGE` em todos os outros campos, para que binário, conta,
//! dependências e gatilhos do serviço nunca sejam reescritos.

use std::ffi::{OsStr, c_void};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use windows::Win32::System::Services::{
    ChangeServiceConfig2W, ChangeServiceConfigW, CloseServiceHandle, ENUM_SERVICE_STATUS_PROCESSW,
    ENUM_SERVICE_TYPE, EnumServicesStatusExW, OpenSCManagerW, OpenServiceW, QUERY_SERVICE_CONFIGW,
    QueryServiceConfig2W, QueryServiceConfigW, QueryServiceStatus, SC_ENUM_PROCESS_INFO, SC_HANDLE,
    SC_MANAGER_CONNECT, SC_MANAGER_ENUMERATE_SERVICE, SERVICE_CHANGE_CONFIG,
    SERVICE_CONFIG_DELAYED_AUTO_START_INFO, SERVICE_DELAYED_AUTO_START_INFO, SERVICE_ERROR,
    SERVICE_QUERY_CONFIG, SERVICE_QUERY_STATUS, SERVICE_START_TYPE, SERVICE_STATE_ALL,
    SERVICE_STATUS, SERVICE_WIN32,
};
use windows::core::PCWSTR;
use windows_service::service::{ServiceAccess, ServiceState};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use crate::error::{AppError, AppResult};

// Códigos de erro Win32 tratados explicitamente.
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
const ERROR_MORE_DATA: u32 = 234;
const ERROR_DEPENDENT_SERVICES_RUNNING: u32 = 1051;
const ERROR_INVALID_SERVICE_CONTROL: u32 = 1052;
const ERROR_SERVICE_ALREADY_RUNNING: u32 = 1056;
const ERROR_SERVICE_DISABLED: u32 = 1058;
const ERROR_SERVICE_DOES_NOT_EXIST: u32 = 1060;
const ERROR_SERVICE_CANNOT_ACCEPT_CTRL: u32 = 1061;
const ERROR_SERVICE_NOT_ACTIVE: u32 = 1062;
const ERROR_SERVICE_MARKED_FOR_DELETE: u32 = 1072;

const SERVICE_NO_CHANGE: u32 = 0xFFFF_FFFF;
/// Bits de `dwServiceType` que identificam serviços por usuário (modelo e instância).
const SERVICE_USER_SERVICE: u32 = 0x40;
const SERVICE_USERSERVICE_INSTANCE: u32 = 0x80;

const ESPERA_MAXIMA: Duration = Duration::from_secs(20);
const INTERVALO_ESPERA: Duration = Duration::from_millis(250);

/// Tipo de inicialização de um serviço.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartType {
    Automatic,
    AutomaticDelayed,
    Manual,
    Disabled,
    /// Boot/System (drivers) ou valor que não pôde ser lido. Nunca é aplicado.
    Unknown,
}

impl StartType {
    pub fn rotulo(self) -> &'static str {
        match self {
            Self::Automatic => "Automático",
            Self::AutomaticDelayed => "Automático (atrasado)",
            Self::Manual => "Manual",
            Self::Disabled => "Desativado",
            Self::Unknown => "Desconhecido",
        }
    }

    pub fn is_automatico(self) -> bool {
        matches!(self, Self::Automatic | Self::AutomaticDelayed)
    }

    fn from_raw(raw: u32, atrasado: bool) -> Self {
        match raw {
            2 if atrasado => Self::AutomaticDelayed,
            2 => Self::Automatic,
            3 => Self::Manual,
            4 => Self::Disabled,
            _ => Self::Unknown,
        }
    }

    fn to_raw(self) -> Option<u32> {
        match self {
            Self::Automatic | Self::AutomaticDelayed => Some(2),
            Self::Manual => Some(3),
            Self::Disabled => Some(4),
            Self::Unknown => None,
        }
    }
}

/// Estado de execução de um serviço.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    Running,
    Stopped,
    StartPending,
    StopPending,
    Paused,
    Unknown,
}

impl ServiceStatus {
    fn from_raw(raw: u32) -> Self {
        match raw {
            1 => Self::Stopped,
            2 => Self::StartPending,
            3 => Self::StopPending,
            4 => Self::Running,
            5..=7 => Self::Paused,
            _ => Self::Unknown,
        }
    }

    /// `true` se o serviço está em execução (ou a caminho disso).
    pub fn is_ativo(self) -> bool {
        matches!(self, Self::Running | Self::StartPending | Self::Paused)
    }
}

/// Dados de um serviço como lidos do SCM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceEntry {
    pub name: String,
    pub display_name: String,
    pub status: ServiceStatus,
    pub start_type: StartType,
    /// Serviço por usuário (modelo ou instância `_xxxxx`).
    pub per_user: bool,
    /// O Windows negou acesso de alteração a este serviço.
    pub acesso_negado: bool,
}

/// Abstração do SCM, para que toda a lógica seja testável com um mock.
pub trait ServiceBackend: Send + Sync {
    /// Lista todos os serviços Win32 (drivers ficam de fora).
    fn list(&self) -> AppResult<Vec<ServiceEntry>>;
    /// Consulta um único serviço.
    fn get(&self, name: &str) -> AppResult<ServiceEntry>;
    /// Altera somente o tipo de inicialização.
    fn set_start_type(&self, name: &str, start_type: StartType) -> AppResult<()>;
    /// Inicia o serviço e aguarda ele entrar em execução.
    fn start(&self, name: &str) -> AppResult<()>;
    /// Para o serviço e aguarda ele parar. Não para dependentes automaticamente.
    fn stop(&self, name: &str) -> AppResult<()>;
}

// ---------------------------------------------------------------------------
// Implementação real (Win32)
// ---------------------------------------------------------------------------

/// Implementação de [`ServiceBackend`] sobre o SCM local.
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowsScm;

/// Handle do SCM fechado automaticamente.
struct ScHandle(SC_HANDLE);

impl Drop for ScHandle {
    fn drop(&mut self) {
        // SAFETY: o handle foi obtido de OpenSCManagerW/OpenServiceW e é fechado uma única vez.
        unsafe {
            let _ = CloseServiceHandle(self.0);
        }
    }
}

fn wide(texto: &str) -> Vec<u16> {
    texto.encode_utf16().chain(std::iter::once(0)).collect()
}

fn codigo_win32(erro: &windows::core::Error) -> u32 {
    // HRESULT_FROM_WIN32: os 16 bits baixos guardam o código Win32 original.
    (erro.code().0 as u32) & 0xFFFF
}

fn descrever_codigo(codigo: u32, padrao: String) -> String {
    match codigo {
        ERROR_DEPENDENT_SERVICES_RUNNING => {
            "outros serviços em execução dependem dele. Pare os dependentes primeiro ou reinicie o computador.".into()
        }
        ERROR_INVALID_SERVICE_CONTROL | ERROR_SERVICE_CANNOT_ACCEPT_CTRL => {
            "o serviço não aceita esse comando agora. A mudança vale após reiniciar o computador.".into()
        }
        ERROR_SERVICE_DISABLED => {
            "o serviço está desativado (ou depende de um dispositivo desativado).".into()
        }
        ERROR_SERVICE_MARKED_FOR_DELETE => "o serviço está marcado para exclusão.".into(),
        _ => format!("{} (código {codigo})", padrao.trim()),
    }
}

fn mapear_erro(servico: &str, operacao: &'static str, codigo: u32, texto: String) -> AppError {
    match codigo {
        ERROR_ACCESS_DENIED => AppError::AcessoNegado(servico.to_string()),
        ERROR_SERVICE_DOES_NOT_EXIST => AppError::ServicoNaoEncontrado(servico.to_string()),
        _ => AppError::Scm {
            servico: servico.to_string(),
            operacao,
            detalhe: descrever_codigo(codigo, texto),
        },
    }
}

fn erro_win(servico: &str, operacao: &'static str, erro: &windows::core::Error) -> AppError {
    mapear_erro(servico, operacao, codigo_win32(erro), erro.message())
}

fn erro_ws(servico: &str, operacao: &'static str, erro: &windows_service::Error) -> AppError {
    match codigo_ws(erro) {
        Some(codigo) => mapear_erro(servico, operacao, codigo, erro.to_string()),
        None => AppError::Scm {
            servico: servico.to_string(),
            operacao,
            detalhe: erro.to_string(),
        },
    }
}

fn codigo_ws(erro: &windows_service::Error) -> Option<u32> {
    match erro {
        windows_service::Error::Winapi(io) => io.raw_os_error().map(|c| c as u32),
        _ => None,
    }
}

fn abrir_gerenciador(acesso: u32) -> AppResult<ScHandle> {
    // SAFETY: ponteiros nulos são válidos aqui (máquina local, base de dados padrão).
    unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), acesso) }
        .map(ScHandle)
        .map_err(|e| AppError::Gerenciador(e.message()))
}

fn abrir_servico(
    gerenciador: &ScHandle,
    nome: &str,
    acesso: u32,
) -> windows::core::Result<ScHandle> {
    let nome_w = wide(nome);
    // SAFETY: `nome_w` é uma string UTF-16 terminada em nulo que vive até o fim da chamada.
    unsafe { OpenServiceW(gerenciador.0, PCWSTR(nome_w.as_ptr()), acesso) }.map(ScHandle)
}

/// Configuração básica lida de um handle com `SERVICE_QUERY_CONFIG`.
struct ConfigLida {
    start_type: StartType,
    display_name: String,
    tipo_servico: u32,
}

fn ler_config(servico: &ScHandle) -> windows::core::Result<ConfigLida> {
    let mut necessario = 0u32;
    // SAFETY: primeira chamada sem buffer, apenas para descobrir o tamanho necessário.
    let primeira = unsafe { QueryServiceConfigW(servico.0, None, 0, &mut necessario) };
    if let Err(erro) = primeira
        && codigo_win32(&erro) != ERROR_INSUFFICIENT_BUFFER
    {
        return Err(erro);
    }

    // Buffer de u64 garante o alinhamento exigido pela struct.
    let mut buffer = vec![0u64; (necessario as usize).div_ceil(8).max(1)];
    let ponteiro = buffer.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
    // SAFETY: o buffer tem pelo menos `necessario` bytes e alinhamento de 8.
    unsafe {
        QueryServiceConfigW(
            servico.0,
            Some(ponteiro),
            (buffer.len() * 8) as u32,
            &mut necessario,
        )?;
    }
    // SAFETY: a chamada acima teve sucesso, então a struct foi preenchida e os ponteiros
    // de string apontam para dentro de `buffer`, que continua vivo.
    let (bruto_start, display_name, tipo_servico) = unsafe {
        let config = &*ponteiro;
        let display = if config.lpDisplayName.is_null() {
            String::new()
        } else {
            config.lpDisplayName.to_string().unwrap_or_default()
        };
        (config.dwStartType.0, display, config.dwServiceType.0)
    };

    Ok(ConfigLida {
        start_type: StartType::from_raw(bruto_start, ler_inicio_atrasado(servico)),
        display_name,
        tipo_servico,
    })
}

fn ler_inicio_atrasado(servico: &ScHandle) -> bool {
    // SERVICE_DELAYED_AUTO_START_INFO é um único BOOL (4 bytes).
    let mut bruto = [0u32; 2];
    let mut necessario = 0u32;
    // SAFETY: fatia de 8 bytes sobre um array de u32 (alinhamento suficiente para BOOL).
    let lido = unsafe {
        let bytes = std::slice::from_raw_parts_mut(bruto.as_mut_ptr().cast::<u8>(), 8);
        QueryServiceConfig2W(
            servico.0,
            SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
            Some(bytes),
            &mut necessario,
        )
    };
    lido.is_ok() && bruto[0] != 0
}

fn eh_por_usuario(tipo_servico: u32) -> bool {
    tipo_servico & (SERVICE_USER_SERVICE | SERVICE_USERSERVICE_INSTANCE) != 0
}

/// Item bruto devolvido pela enumeração.
struct ItemEnumerado {
    name: String,
    display_name: String,
    status: ServiceStatus,
    tipo_servico: u32,
}

fn enumerar(gerenciador: &ScHandle) -> AppResult<Vec<ItemEnumerado>> {
    let mut itens = Vec::new();
    let mut buffer = vec![0u64; 8 * 1024]; // 64 KiB, alinhado em 8 bytes
    let mut retomar = 0u32;

    loop {
        let mut necessario = 0u32;
        let mut retornados = 0u32;
        // SAFETY: a fatia cobre exatamente a memória de `buffer`; os ponteiros de saída
        // são variáveis locais válidas durante a chamada.
        let resultado = unsafe {
            let bytes =
                std::slice::from_raw_parts_mut(buffer.as_mut_ptr().cast::<u8>(), buffer.len() * 8);
            EnumServicesStatusExW(
                gerenciador.0,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                Some(bytes),
                &mut necessario,
                &mut retornados,
                Some(&mut retomar),
                PCWSTR::null(),
            )
        };

        let ha_mais = match resultado {
            Ok(()) => false,
            Err(erro) if codigo_win32(&erro) == ERROR_MORE_DATA => true,
            Err(erro) => return Err(AppError::Gerenciador(erro.message())),
        };

        // SAFETY: o SCM preencheu `retornados` structs no início do buffer; as strings
        // apontam para dentro do mesmo buffer e são copiadas antes da próxima chamada.
        unsafe {
            let entradas = std::slice::from_raw_parts(
                buffer.as_ptr().cast::<ENUM_SERVICE_STATUS_PROCESSW>(),
                retornados as usize,
            );
            for entrada in entradas {
                if entrada.lpServiceName.is_null() {
                    continue;
                }
                let name = entrada.lpServiceName.to_string().unwrap_or_default();
                if name.is_empty() {
                    continue;
                }
                let display_name = if entrada.lpDisplayName.is_null() {
                    name.clone()
                } else {
                    entrada.lpDisplayName.to_string().unwrap_or_default()
                };
                itens.push(ItemEnumerado {
                    name,
                    display_name,
                    status: ServiceStatus::from_raw(entrada.ServiceStatusProcess.dwCurrentState.0),
                    tipo_servico: entrada.ServiceStatusProcess.dwServiceType.0,
                });
            }
        }

        if !ha_mais {
            break;
        }
        if retornados == 0 {
            // Nem um item coube: aumenta o buffer para o tamanho pedido pelo SCM.
            let novo = (necessario as usize).div_ceil(8).max(buffer.len() * 2);
            buffer = vec![0u64; novo];
        }
    }

    Ok(itens)
}

fn esperar_estado(
    servico: &windows_service::service::Service,
    nome: &str,
    alvo: ServiceState,
    operacao: &'static str,
) -> AppResult<()> {
    let inicio = Instant::now();
    loop {
        let estado = servico
            .query_status()
            .map_err(|e| erro_ws(nome, operacao, &e))?
            .current_state;
        if estado == alvo {
            return Ok(());
        }
        if inicio.elapsed() >= ESPERA_MAXIMA {
            return Err(AppError::Scm {
                servico: nome.to_string(),
                operacao,
                detalhe: format!(
                    "o serviço não respondeu em {} segundos.",
                    ESPERA_MAXIMA.as_secs()
                ),
            });
        }
        std::thread::sleep(INTERVALO_ESPERA);
    }
}

fn abrir_ws(
    nome: &str,
    acesso: ServiceAccess,
    operacao: &'static str,
) -> AppResult<windows_service::service::Service> {
    let gerenciador = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| AppError::Gerenciador(e.to_string()))?;
    gerenciador
        .open_service(nome, acesso)
        .map_err(|e| erro_ws(nome, operacao, &e))
}

impl ServiceBackend for WindowsScm {
    fn list(&self) -> AppResult<Vec<ServiceEntry>> {
        let gerenciador = abrir_gerenciador(SC_MANAGER_CONNECT | SC_MANAGER_ENUMERATE_SERVICE)?;
        let enumerados = enumerar(&gerenciador)?;

        let mut servicos = Vec::with_capacity(enumerados.len());
        for item in enumerados {
            let (start_type, leitura_negada) =
                match abrir_servico(&gerenciador, &item.name, SERVICE_QUERY_CONFIG)
                    .and_then(|handle| ler_config(&handle))
                {
                    Ok(config) => (config.start_type, false),
                    Err(_) => (StartType::Unknown, true),
                };

            // Abrir com SERVICE_CHANGE_CONFIG não altera nada: só revela se o Windows
            // permitiria a alteração (serviços protegidos devolvem ACCESS_DENIED).
            let acesso_negado = leitura_negada
                || matches!(
                    abrir_servico(&gerenciador, &item.name, SERVICE_CHANGE_CONFIG),
                    Err(erro) if codigo_win32(&erro) == ERROR_ACCESS_DENIED
                );

            servicos.push(ServiceEntry {
                per_user: eh_por_usuario(item.tipo_servico),
                name: item.name,
                display_name: item.display_name,
                status: item.status,
                start_type,
                acesso_negado,
            });
        }

        servicos.sort_by_key(|s| s.display_name.to_lowercase());
        Ok(servicos)
    }

    fn get(&self, name: &str) -> AppResult<ServiceEntry> {
        const OPERACAO: &str = "consultar";
        let gerenciador = abrir_gerenciador(SC_MANAGER_CONNECT)?;
        let handle = abrir_servico(
            &gerenciador,
            name,
            SERVICE_QUERY_CONFIG | SERVICE_QUERY_STATUS,
        )
        .map_err(|e| erro_win(name, OPERACAO, &e))?;
        let config = ler_config(&handle).map_err(|e| erro_win(name, OPERACAO, &e))?;

        let mut status = SERVICE_STATUS::default();
        // SAFETY: `status` é uma struct local válida para escrita.
        unsafe { QueryServiceStatus(handle.0, &mut status) }
            .map_err(|e| erro_win(name, OPERACAO, &e))?;

        let acesso_negado = matches!(
            abrir_servico(&gerenciador, name, SERVICE_CHANGE_CONFIG),
            Err(erro) if codigo_win32(&erro) == ERROR_ACCESS_DENIED
        );

        Ok(ServiceEntry {
            name: name.to_string(),
            display_name: if config.display_name.is_empty() {
                name.to_string()
            } else {
                config.display_name
            },
            status: ServiceStatus::from_raw(status.dwCurrentState.0),
            start_type: config.start_type,
            per_user: eh_por_usuario(config.tipo_servico),
            acesso_negado,
        })
    }

    fn set_start_type(&self, name: &str, start_type: StartType) -> AppResult<()> {
        const OPERACAO: &str = "alterar o tipo de inicialização d";
        let bruto = start_type.to_raw().ok_or_else(|| {
            AppError::Invalido("Tipo de inicialização inválido para aplicação.".into())
        })?;

        let gerenciador = abrir_gerenciador(SC_MANAGER_CONNECT)?;
        let handle = abrir_servico(&gerenciador, name, SERVICE_CHANGE_CONFIG)
            .map_err(|e| erro_win(name, OPERACAO, &e))?;

        // SAFETY: todos os campos além do start type usam SERVICE_NO_CHANGE / nulo,
        // o que instrui o SCM a preservar a configuração existente.
        unsafe {
            ChangeServiceConfigW(
                handle.0,
                ENUM_SERVICE_TYPE(SERVICE_NO_CHANGE),
                SERVICE_START_TYPE(bruto),
                SERVICE_ERROR(SERVICE_NO_CHANGE),
                PCWSTR::null(),
                PCWSTR::null(),
                None,
                PCWSTR::null(),
                PCWSTR::null(),
                PCWSTR::null(),
                PCWSTR::null(),
            )
        }
        .map_err(|e| erro_win(name, OPERACAO, &e))?;

        if start_type.is_automatico() {
            let atrasado = start_type == StartType::AutomaticDelayed;
            let info = SERVICE_DELAYED_AUTO_START_INFO {
                fDelayedAutostart: atrasado.into(),
            };
            // SAFETY: `info` vive até o fim da chamada e tem o layout esperado pelo SCM.
            let resultado = unsafe {
                ChangeServiceConfig2W(
                    handle.0,
                    SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
                    Some(std::ptr::from_ref(&info).cast::<c_void>()),
                )
            };
            // Limpar o atraso pode falhar em serviços que não o suportam; só é erro
            // quando o atraso foi pedido explicitamente.
            if let Err(erro) = resultado
                && atrasado
            {
                return Err(erro_win(name, OPERACAO, &erro));
            }
        }
        Ok(())
    }

    fn start(&self, name: &str) -> AppResult<()> {
        const OPERACAO: &str = "iniciar";
        let servico = abrir_ws(
            name,
            ServiceAccess::START | ServiceAccess::QUERY_STATUS,
            OPERACAO,
        )?;
        match servico.start::<&OsStr>(&[]) {
            Ok(()) => {}
            Err(erro) if codigo_ws(&erro) == Some(ERROR_SERVICE_ALREADY_RUNNING) => return Ok(()),
            Err(erro) => return Err(erro_ws(name, OPERACAO, &erro)),
        }
        esperar_estado(&servico, name, ServiceState::Running, OPERACAO)
    }

    fn stop(&self, name: &str) -> AppResult<()> {
        const OPERACAO: &str = "parar";
        let servico = abrir_ws(
            name,
            ServiceAccess::STOP | ServiceAccess::QUERY_STATUS,
            OPERACAO,
        )?;
        match servico.stop() {
            Ok(_) => {}
            Err(erro) if codigo_ws(&erro) == Some(ERROR_SERVICE_NOT_ACTIVE) => return Ok(()),
            Err(erro) => return Err(erro_ws(name, OPERACAO, &erro)),
        }
        esperar_estado(&servico, name, ServiceState::Stopped, OPERACAO)
    }
}

// ---------------------------------------------------------------------------
// Mock para testes
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod mock {
    use std::collections::{BTreeMap, HashSet};
    use std::sync::Mutex;

    use super::*;

    /// Backend em memória. Nomes são comparados sem diferenciar maiúsculas, como no SCM.
    #[derive(Default)]
    pub struct MockBackend {
        servicos: Mutex<BTreeMap<String, ServiceEntry>>,
        negados: Mutex<HashSet<String>>,
        nao_param: Mutex<HashSet<String>>,
        chamadas: Mutex<Vec<String>>,
    }

    impl MockBackend {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn com(mut self_: Self, name: &str, start_type: StartType, rodando: bool) -> Self {
            self_.inserir(ServiceEntry {
                name: name.to_string(),
                display_name: format!("Serviço {name}"),
                status: if rodando {
                    ServiceStatus::Running
                } else {
                    ServiceStatus::Stopped
                },
                start_type,
                per_user: false,
                acesso_negado: false,
            });
            self_
        }

        pub fn inserir(&mut self, entrada: ServiceEntry) {
            self.servicos
                .lock()
                .unwrap()
                .insert(entrada.name.to_lowercase(), entrada);
        }

        /// Faz o serviço responder ACCESS_DENIED a qualquer alteração.
        pub fn negar_acesso(&self, name: &str) {
            self.negados.lock().unwrap().insert(name.to_lowercase());
            if let Some(s) = self.servicos.lock().unwrap().get_mut(&name.to_lowercase()) {
                s.acesso_negado = true;
            }
        }

        /// Faz o serviço recusar o comando de parada.
        pub fn recusar_parada(&self, name: &str) {
            self.nao_param.lock().unwrap().insert(name.to_lowercase());
        }

        /// Chamadas de alteração recebidas, na ordem (`set:Nome:tipo`, `start:Nome`, `stop:Nome`).
        pub fn chamadas(&self) -> Vec<String> {
            self.chamadas.lock().unwrap().clone()
        }

        fn alterar<T>(
            &self,
            name: &str,
            acao: impl FnOnce(&mut ServiceEntry) -> T,
        ) -> AppResult<T> {
            let chave = name.to_lowercase();
            if self.negados.lock().unwrap().contains(&chave) {
                return Err(AppError::AcessoNegado(name.to_string()));
            }
            let mut servicos = self.servicos.lock().unwrap();
            let servico = servicos
                .get_mut(&chave)
                .ok_or_else(|| AppError::ServicoNaoEncontrado(name.to_string()))?;
            Ok(acao(servico))
        }
    }

    impl ServiceBackend for MockBackend {
        fn list(&self) -> AppResult<Vec<ServiceEntry>> {
            Ok(self.servicos.lock().unwrap().values().cloned().collect())
        }

        fn get(&self, name: &str) -> AppResult<ServiceEntry> {
            self.servicos
                .lock()
                .unwrap()
                .get(&name.to_lowercase())
                .cloned()
                .ok_or_else(|| AppError::ServicoNaoEncontrado(name.to_string()))
        }

        fn set_start_type(&self, name: &str, start_type: StartType) -> AppResult<()> {
            self.alterar(name, |s| s.start_type = start_type)?;
            self.chamadas
                .lock()
                .unwrap()
                .push(format!("set:{name}:{start_type:?}"));
            Ok(())
        }

        fn start(&self, name: &str) -> AppResult<()> {
            let desativado = self.alterar(name, |s| {
                if s.start_type == StartType::Disabled {
                    true
                } else {
                    s.status = ServiceStatus::Running;
                    false
                }
            })?;
            if desativado {
                return Err(AppError::Scm {
                    servico: name.to_string(),
                    operacao: "iniciar",
                    detalhe: "o serviço está desativado.".into(),
                });
            }
            self.chamadas.lock().unwrap().push(format!("start:{name}"));
            Ok(())
        }

        fn stop(&self, name: &str) -> AppResult<()> {
            if self
                .nao_param
                .lock()
                .unwrap()
                .contains(&name.to_lowercase())
            {
                return Err(AppError::Scm {
                    servico: name.to_string(),
                    operacao: "parar",
                    detalhe: "o serviço não aceita esse comando agora.".into(),
                });
            }
            self.alterar(name, |s| s.status = ServiceStatus::Stopped)?;
            self.chamadas.lock().unwrap().push(format!("stop:{name}"));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::MockBackend;
    use super::*;

    #[test]
    fn start_type_converte_valores_brutos() {
        assert_eq!(StartType::from_raw(2, false), StartType::Automatic);
        assert_eq!(StartType::from_raw(2, true), StartType::AutomaticDelayed);
        assert_eq!(StartType::from_raw(3, false), StartType::Manual);
        assert_eq!(StartType::from_raw(3, true), StartType::Manual);
        assert_eq!(StartType::from_raw(4, false), StartType::Disabled);
        assert_eq!(StartType::from_raw(0, false), StartType::Unknown);
        assert_eq!(StartType::from_raw(1, false), StartType::Unknown);
    }

    #[test]
    fn start_type_unknown_nunca_e_aplicavel() {
        assert_eq!(StartType::Unknown.to_raw(), None);
        assert_eq!(StartType::Automatic.to_raw(), Some(2));
        assert_eq!(StartType::AutomaticDelayed.to_raw(), Some(2));
        assert_eq!(StartType::Manual.to_raw(), Some(3));
        assert_eq!(StartType::Disabled.to_raw(), Some(4));
    }

    #[test]
    fn start_type_serializa_em_snake_case() {
        let json = serde_json::to_string(&StartType::AutomaticDelayed).unwrap();
        assert_eq!(json, "\"automatic_delayed\"");
        let volta: StartType = serde_json::from_str("\"disabled\"").unwrap();
        assert_eq!(volta, StartType::Disabled);
    }

    #[test]
    fn status_converte_valores_brutos() {
        assert_eq!(ServiceStatus::from_raw(1), ServiceStatus::Stopped);
        assert_eq!(ServiceStatus::from_raw(4), ServiceStatus::Running);
        assert_eq!(ServiceStatus::from_raw(7), ServiceStatus::Paused);
        assert_eq!(ServiceStatus::from_raw(99), ServiceStatus::Unknown);
        assert!(ServiceStatus::Running.is_ativo());
        assert!(!ServiceStatus::Stopped.is_ativo());
    }

    #[test]
    fn detecta_servico_por_usuario_pelo_tipo() {
        assert!(eh_por_usuario(0xE0)); // instância de serviço por usuário
        assert!(eh_por_usuario(0x60)); // modelo de serviço por usuário
        assert!(!eh_por_usuario(0x10)); // processo próprio
        assert!(!eh_por_usuario(0x20)); // processo compartilhado
    }

    #[test]
    fn erros_win32_viram_mensagens_claras() {
        let negado = mapear_erro("Foo", "parar", ERROR_ACCESS_DENIED, String::new());
        assert!(matches!(negado, AppError::AcessoNegado(ref n) if n == "Foo"));

        let ausente = mapear_erro("Foo", "parar", ERROR_SERVICE_DOES_NOT_EXIST, String::new());
        assert!(matches!(ausente, AppError::ServicoNaoEncontrado(_)));

        let dependentes = mapear_erro(
            "Foo",
            "parar",
            ERROR_DEPENDENT_SERVICES_RUNNING,
            String::new(),
        );
        let texto = dependentes.to_string();
        assert!(texto.contains("parar"), "{texto}");
        assert!(texto.contains("dependem"), "{texto}");
    }

    #[test]
    fn mock_altera_start_type_e_registra_chamadas() {
        let mock = MockBackend::com(MockBackend::new(), "DiagTrack", StartType::Automatic, true);
        mock.set_start_type("diagtrack", StartType::Disabled)
            .unwrap();
        mock.stop("DiagTrack").unwrap();

        let servico = mock.get("DIAGTRACK").unwrap();
        assert_eq!(servico.start_type, StartType::Disabled);
        assert_eq!(servico.status, ServiceStatus::Stopped);
        assert_eq!(
            mock.chamadas(),
            vec!["set:diagtrack:Disabled", "stop:DiagTrack"]
        );
    }

    #[test]
    fn mock_devolve_erros_esperados() {
        let mock = MockBackend::com(MockBackend::new(), "Fax", StartType::Disabled, false);
        assert!(matches!(
            mock.get("NaoExiste"),
            Err(AppError::ServicoNaoEncontrado(_))
        ));
        assert!(mock.start("Fax").is_err(), "serviço desativado não inicia");

        mock.negar_acesso("Fax");
        assert!(matches!(
            mock.set_start_type("Fax", StartType::Manual),
            Err(AppError::AcessoNegado(_))
        ));
        assert!(mock.chamadas().is_empty());
    }

    /// Teste somente-leitura contra o SCM real: valida o código `unsafe` da enumeração.
    #[test]
    fn scm_real_lista_servicos_sem_alterar_nada() {
        let servicos = WindowsScm.list().unwrap();
        assert!(servicos.len() > 50, "poucos serviços: {}", servicos.len());

        let rpc = servicos
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case("RpcSs"))
            .expect("RpcSs deveria existir em qualquer Windows");
        assert_eq!(rpc.status, ServiceStatus::Running);
        assert_eq!(rpc.start_type, StartType::Automatic);
        assert!(!rpc.display_name.is_empty());

        let individual = WindowsScm.get("RpcSs").unwrap();
        assert_eq!(individual.start_type, rpc.start_type);
        assert_eq!(individual.display_name, rpc.display_name);

        assert!(matches!(
            WindowsScm.get("WinTuneServicoQueNaoExiste"),
            Err(AppError::ServicoNaoEncontrado(_))
        ));

        let por_usuario = servicos.iter().filter(|s| s.per_user).count();
        let negados = servicos.iter().filter(|s| s.acesso_negado).count();
        let desconhecidos = servicos
            .iter()
            .filter(|s| s.start_type == StartType::Unknown)
            .count();
        println!(
            "SCM real: {} serviços, {por_usuario} por usuário, {negados} com acesso negado, {desconhecidos} com start type desconhecido",
            servicos.len()
        );
    }
}
