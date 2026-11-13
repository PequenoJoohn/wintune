//! Efeitos visuais do Windows (a lista de "Opções de desempenho → Efeitos visuais" e a
//! transparência do Windows 10/11).
//!
//! Diferente dos serviços, efeitos visuais são configurações **do usuário** (HKCU e
//! `SystemParametersInfo`). Como o app roda elevado, ele só altera efeitos quando o
//! processo pertence ao mesmo usuário da área de trabalho (ver
//! [`EffectsBackend::verificar_usuario`]); caso contrário as mudanças iriam para o perfil
//! da conta de administrador, e não para quem está usando o PC.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::logger::Logger;
use crate::ops::{ApplyReport, ItemRelatorio};
use crate::scm::ServiceBackend;
use crate::snapshot::{EfeitosSalvos, EstadoEfeito, Snapshot, SnapshotKind, SnapshotStore};

/// Valor de `VisualFXSetting` que o Windows usa para "Personalizado".
pub const MODO_PERSONALIZADO: u32 = 3;

const CHAVE_AVANCADO: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
const CHAVE_DWM: &str = r"Software\Microsoft\Windows\DWM";
const CHAVE_PERSONALIZAR: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
pub const CHAVE_MODO: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects";
pub const VALOR_MODO: &str = "VisualFXSetting";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Impacto {
    Baixo,
    Medio,
    Alto,
}

/// Como o efeito é lido e gravado no Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mecanismo {
    /// `SystemParametersInfoW`: leitura de um BOOL em `pvParam`, gravação com o valor
    /// passado no próprio ponteiro `pvParam`.
    SpiPv { ler: u32, gravar: u32 },
    /// Como `SpiPv`, mas a gravação usa `uiParam`.
    SpiUi { ler: u32, gravar: u32 },
    /// `SPI_GETANIMATION`/`SPI_SETANIMATION` (struct `ANIMATIONINFO`).
    AnimacaoJanelas,
    /// DWORD em `HKCU\<chave>`. `padrao` é o estado quando o valor não existe.
    Registro {
        chave: &'static str,
        valor: &'static str,
        ligado: u32,
        desligado: u32,
        padrao: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Efeito {
    pub id: &'static str,
    pub nome: &'static str,
    pub descricao: &'static str,
    pub impacto: Impacto,
    /// O WinTune recomenda manter ligado (o ganho é mínimo e a perda de usabilidade, grande).
    pub manter_ligado: bool,
    /// Só vale por completo depois de sair e entrar de novo na conta.
    pub requer_logoff: bool,
    pub mecanismo: Mecanismo,
}

const fn spi_pv(ler: u32, gravar: u32) -> Mecanismo {
    Mecanismo::SpiPv { ler, gravar }
}

const fn reg(chave: &'static str, valor: &'static str) -> Mecanismo {
    Mecanismo::Registro {
        chave,
        valor,
        ligado: 1,
        desligado: 0,
        padrao: true,
    }
}

/// Todos os efeitos que o WinTune controla, na ordem em que aparecem na tela.
pub const EFEITOS: &[Efeito] = &[
    Efeito {
        id: "transparencia",
        nome: "Efeitos de transparência",
        descricao: "Fundo translúcido do menu Iniciar, da barra de tarefas e de janelas do Windows 11. Desligado, essas áreas ficam com cor sólida e a placa de vídeo trabalha menos.",
        impacto: Impacto::Medio,
        manter_ligado: false,
        requer_logoff: true,
        mecanismo: reg(CHAVE_PERSONALIZAR, "EnableTransparency"),
    },
    Efeito {
        id: "animar_janelas",
        nome: "Animar janelas ao minimizar e maximizar",
        descricao: "Animação de zoom ao minimizar, maximizar e restaurar janelas. Desligado, as janelas mudam de tamanho instantaneamente.",
        impacto: Impacto::Alto,
        manter_ligado: false,
        requer_logoff: false,
        mecanismo: Mecanismo::AnimacaoJanelas,
    },
    Efeito {
        id: "animar_controles",
        nome: "Animar controles e elementos dentro de janelas",
        descricao: "Animações de botões, barras de rolagem e transições do app Configurações e de outros apps. Corresponde a \"Efeitos de animação\" em Acessibilidade.",
        impacto: Impacto::Medio,
        manter_ligado: false,
        requer_logoff: false,
        mecanismo: spi_pv(0x1042, 0x1043),
    },
    Efeito {
        id: "animacoes_barra",
        nome: "Animações na barra de tarefas",
        descricao: "Animações ao abrir e fechar programas e ao passar o mouse na barra de tarefas.",
        impacto: Impacto::Medio,
        manter_ligado: false,
        requer_logoff: true,
        mecanismo: reg(CHAVE_AVANCADO, "TaskbarAnimations"),
    },
    Efeito {
        id: "sombra_janelas",
        nome: "Mostrar sombras sob as janelas",
        descricao: "Sombra em volta das janelas. Desligado, as bordas ficam mais difíceis de distinguir quando há janelas sobrepostas.",
        impacto: Impacto::Medio,
        manter_ligado: false,
        requer_logoff: false,
        mecanismo: spi_pv(0x1024, 0x1025),
    },
    Efeito {
        id: "arrastar_conteudo",
        nome: "Mostrar conteúdo da janela ao arrastar",
        descricao: "Mostra a janela inteira enquanto ela é movida. Desligado, só um contorno acompanha o mouse. Recomendado manter: o ganho é mínimo em PCs atuais.",
        impacto: Impacto::Medio,
        manter_ligado: true,
        requer_logoff: false,
        mecanismo: Mecanismo::SpiUi {
            ler: 0x0026,
            gravar: 0x0025,
        },
    },
    Efeito {
        id: "miniaturas",
        nome: "Mostrar miniaturas em vez de ícones",
        descricao: "Prévia de fotos e vídeos no Explorador de Arquivos. Desligado, aparecem só ícones genéricos. Recomendado manter, a não ser em PCs muito lentos.",
        impacto: Impacto::Medio,
        manter_ligado: true,
        requer_logoff: true,
        mecanismo: Mecanismo::Registro {
            chave: CHAVE_AVANCADO,
            valor: "IconsOnly",
            ligado: 0,
            desligado: 1,
            padrao: true,
        },
    },
    Efeito {
        id: "aero_peek",
        nome: "Habilitar Espiar (Peek)",
        descricao: "Mostra a área de trabalho ao passar o mouse no canto da barra de tarefas e a prévia em tela cheia de janelas.",
        impacto: Impacto::Baixo,
        manter_ligado: false,
        requer_logoff: true,
        mecanismo: reg(CHAVE_DWM, "EnableAeroPeek"),
    },
    Efeito {
        id: "salvar_miniaturas",
        nome: "Salvar visualizações das miniaturas da barra de tarefas",
        descricao: "Guarda em memória a prévia das janelas minimizadas. Desligado (padrão do Windows), economiza um pouco de RAM.",
        impacto: Impacto::Baixo,
        manter_ligado: false,
        requer_logoff: true,
        mecanismo: Mecanismo::Registro {
            chave: CHAVE_DWM,
            valor: "AlwaysHibernateThumbnails",
            ligado: 1,
            desligado: 0,
            padrao: false,
        },
    },
    Efeito {
        id: "menus_fade",
        nome: "Fazer menus aparecerem gradualmente ou deslizarem",
        descricao: "Animação de abertura dos menus. Desligado, os menus aparecem na hora.",
        impacto: Impacto::Baixo,
        manter_ligado: false,
        requer_logoff: false,
        mecanismo: spi_pv(0x1002, 0x1003),
    },
    Efeito {
        id: "dicas_fade",
        nome: "Fazer dicas de ferramenta aparecerem gradualmente ou deslizarem",
        descricao: "Animação das dicas que aparecem ao parar o mouse sobre um item.",
        impacto: Impacto::Baixo,
        manter_ligado: false,
        requer_logoff: false,
        mecanismo: spi_pv(0x1016, 0x1017),
    },
    Efeito {
        id: "itens_menu_fade",
        nome: "Esmaecer itens de menu após clicar",
        descricao: "O item clicado some aos poucos em vez de sumir na hora.",
        impacto: Impacto::Baixo,
        manter_ligado: false,
        requer_logoff: false,
        mecanismo: spi_pv(0x1014, 0x1015),
    },
    Efeito {
        id: "combo_deslizar",
        nome: "Deslizar caixas de combinação abertas",
        descricao: "Animação ao abrir listas suspensas.",
        impacto: Impacto::Baixo,
        manter_ligado: false,
        requer_logoff: false,
        mecanismo: spi_pv(0x1004, 0x1005),
    },
    Efeito {
        id: "rolagem_suave",
        nome: "Rolagem suave em caixas de listagem",
        descricao: "Rolagem animada em listas de programas clássicos.",
        impacto: Impacto::Baixo,
        manter_ligado: false,
        requer_logoff: false,
        mecanismo: spi_pv(0x1006, 0x1007),
    },
    Efeito {
        id: "sombra_ponteiro",
        nome: "Mostrar sombras sob o ponteiro do mouse",
        descricao: "Pequena sombra sob a seta do mouse.",
        impacto: Impacto::Baixo,
        manter_ligado: false,
        requer_logoff: false,
        mecanismo: spi_pv(0x101A, 0x101B),
    },
    Efeito {
        id: "selecao_translucida",
        nome: "Mostrar retângulo de seleção translúcido",
        descricao: "Retângulo azul translúcido ao selecionar arquivos arrastando o mouse. Desligado, aparece só o contorno.",
        impacto: Impacto::Baixo,
        manter_ligado: false,
        requer_logoff: true,
        mecanismo: reg(CHAVE_AVANCADO, "ListviewAlphaSelect"),
    },
    Efeito {
        id: "sombra_icones",
        nome: "Usar sombras subjacentes para rótulos de ícones na área de trabalho",
        descricao: "Sombra sob o nome dos ícones da área de trabalho, que ajuda a ler sobre papéis de parede claros.",
        impacto: Impacto::Baixo,
        manter_ligado: false,
        requer_logoff: true,
        mecanismo: reg(CHAVE_AVANCADO, "ListviewShadow"),
    },
    Efeito {
        id: "suavizar_fontes",
        nome: "Suavizar bordas das fontes de tela",
        descricao: "Suavização (ClearType) do texto. Desligado, o texto fica serrilhado e difícil de ler. Recomendado manter sempre ligado.",
        impacto: Impacto::Baixo,
        manter_ligado: true,
        requer_logoff: false,
        mecanismo: Mecanismo::SpiUi {
            ler: 0x004A,
            gravar: 0x004B,
        },
    },
];

pub fn buscar(id: &str) -> AppResult<&'static Efeito> {
    EFEITOS
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| AppError::Invalido(format!("Efeito visual \"{id}\" desconhecido.")))
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

pub trait EffectsBackend: Send + Sync + std::fmt::Debug {
    /// Garante que o processo é do mesmo usuário da área de trabalho. Erro se não for.
    fn verificar_usuario(&self) -> AppResult<()>;
    fn ler(&self, efeito: &Efeito) -> AppResult<bool>;
    fn definir(&self, efeito: &Efeito, ligado: bool) -> AppResult<()>;
    /// `VisualFXSetting` atual (`None` se o valor não existe).
    fn ler_modo(&self) -> AppResult<Option<u32>>;
    fn definir_modo(&self, modo: u32) -> AppResult<()>;
    /// Avisa os programas abertos de que as configurações mudaram.
    fn notificar(&self);
}

/// Lê o estado de todos os efeitos. Efeitos que não puderam ser lidos ficam de fora.
pub fn capturar(backend: &dyn EffectsBackend) -> AppResult<EfeitosSalvos> {
    backend.verificar_usuario()?;
    Ok(EfeitosSalvos {
        modo_windows: backend.ler_modo().ok().flatten(),
        estados: EFEITOS
            .iter()
            .filter_map(|e| {
                backend.ler(e).ok().map(|ligado| EstadoEfeito {
                    id: e.id.to_string(),
                    ligado,
                })
            })
            .collect(),
    })
}

#[cfg(windows)]
pub use windows_impl::WindowsEfeitos;

#[cfg(windows)]
mod windows_impl {
    use std::ffi::c_void;

    use windows::Win32::Foundation::{CloseHandle, ERROR_FILE_NOT_FOUND, HANDLE, LPARAM, WPARAM};
    use windows::Win32::Security::{
        GetLengthSid, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW,
    };
    use windows::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        ANIMATIONINFO, GetShellWindow, GetWindowThreadProcessId, HWND_BROADCAST, SMTO_ABORTIFHUNG,
        SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SYSTEM_PARAMETERS_INFO_ACTION,
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SendMessageTimeoutW, SystemParametersInfoW,
        WM_SETTINGCHANGE,
    };
    use windows::core::HSTRING;

    use super::{CHAVE_MODO, Efeito, EffectsBackend, Mecanismo, VALOR_MODO};
    use crate::error::{AppError, AppResult};

    const SPI_GETANIMATION: u32 = 0x0048;
    const SPI_SETANIMATION: u32 = 0x0049;

    #[derive(Debug, Default)]
    pub struct WindowsEfeitos;

    fn erro(efeito: &str, operacao: &str, e: &windows::core::Error) -> AppError {
        AppError::Outro(format!(
            "Não foi possível {operacao} o efeito \"{efeito}\": {}",
            e.message()
        ))
    }

    fn gravar_ini() -> SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS {
        SPIF_UPDATEINIFILE | SPIF_SENDCHANGE
    }

    fn ler_dword(chave: &str, valor: &str) -> windows::core::Result<Option<u32>> {
        let mut dados = 0u32;
        let mut tamanho = size_of::<u32>() as u32;
        // SAFETY: o buffer é um u32 local e `tamanho` informa seu tamanho.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(chave),
                &HSTRING::from(valor),
                RRF_RT_REG_DWORD,
                None,
                Some(std::ptr::from_mut(&mut dados).cast::<c_void>()),
                Some(&mut tamanho),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        status.ok().map(|()| Some(dados))
    }

    fn gravar_dword(chave: &str, valor: &str, dados: u32) -> windows::core::Result<()> {
        // SAFETY: os dados são um u32 local e o tamanho informado é o de um u32.
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(chave),
                &HSTRING::from(valor),
                REG_DWORD.0,
                Some(std::ptr::from_ref(&dados).cast::<c_void>()),
                size_of::<u32>() as u32,
            )
        }
        .ok()
    }

    /// SID (em bytes) do usuário dono do token de um processo.
    fn sid_do_processo(processo: HANDLE) -> windows::core::Result<Vec<u8>> {
        let mut token = HANDLE::default();
        // SAFETY: `processo` é um handle válido com direito de consulta; `token` é local.
        unsafe { OpenProcessToken(processo, TOKEN_QUERY, &mut token) }?;

        let mut tamanho = 0u32;
        // Primeira chamada só descobre o tamanho necessário (falha esperada).
        // SAFETY: buffer nulo com tamanho 0 é o uso documentado para consultar o tamanho.
        let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut tamanho) };
        // Vec<u64> garante o alinhamento de TOKEN_USER.
        let mut buffer = vec![0u64; (tamanho as usize).div_ceil(8).max(1)];
        // SAFETY: o buffer tem pelo menos `tamanho` bytes e o token é fechado em seguida.
        let resultado = unsafe {
            let r = GetTokenInformation(
                token,
                TokenUser,
                Some(buffer.as_mut_ptr().cast::<c_void>()),
                tamanho,
                &mut tamanho,
            );
            let _ = CloseHandle(token);
            r
        };
        resultado?;

        // SAFETY: GetTokenInformation(TokenUser) preencheu um TOKEN_USER no início do buffer,
        // e o SID apontado vive dentro do mesmo buffer.
        unsafe {
            let usuario = &*buffer.as_ptr().cast::<TOKEN_USER>();
            let sid = usuario.User.Sid;
            let tamanho_sid = GetLengthSid(sid) as usize;
            Ok(std::slice::from_raw_parts(sid.0.cast::<u8>(), tamanho_sid).to_vec())
        }
    }

    impl EffectsBackend for WindowsEfeitos {
        fn verificar_usuario(&self) -> AppResult<()> {
            let falha = || {
                AppError::Outro(
                    "Não foi possível confirmar qual usuário está usando a área de trabalho, então os efeitos visuais não serão alterados. Verifique se o Explorador de Arquivos está aberto."
                        .into(),
                )
            };

            // SAFETY: chamadas sem ponteiros além do `pid` local.
            let shell = unsafe { GetShellWindow() };
            if shell.is_invalid() {
                return Err(falha());
            }
            let mut pid = 0u32;
            // SAFETY: `shell` é uma janela válida e `pid` é local.
            unsafe { GetWindowThreadProcessId(shell, Some(&mut pid)) };
            if pid == 0 {
                return Err(falha());
            }

            // SAFETY: handle aberto aqui e fechado logo após o uso.
            let sid_shell = unsafe {
                let processo = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)
                    .map_err(|_| falha())?;
                let sid = sid_do_processo(processo);
                let _ = CloseHandle(processo);
                sid.map_err(|_| falha())?
            };
            // SAFETY: GetCurrentProcess devolve um pseudo-handle sempre válido.
            let sid_proprio =
                sid_do_processo(unsafe { GetCurrentProcess() }).map_err(|_| falha())?;

            if sid_shell == sid_proprio {
                Ok(())
            } else {
                Err(AppError::Outro(
                    "O WinTune foi aberto com outra conta de administrador. Efeitos visuais são configurações de cada usuário e seriam aplicados na conta errada, então foram bloqueados. Para alterá-los, entre com uma conta que seja administradora e abra o WinTune por ela."
                        .into(),
                ))
            }
        }

        fn ler(&self, efeito: &Efeito) -> AppResult<bool> {
            match efeito.mecanismo {
                Mecanismo::SpiPv { ler, .. } | Mecanismo::SpiUi { ler, .. } => {
                    let mut valor = 0i32;
                    // SAFETY: as ações de leitura usadas gravam um BOOL (i32) em pvParam.
                    unsafe {
                        SystemParametersInfoW(
                            SYSTEM_PARAMETERS_INFO_ACTION(ler),
                            0,
                            Some(std::ptr::from_mut(&mut valor).cast::<c_void>()),
                            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
                        )
                    }
                    .map_err(|e| erro(efeito.nome, "ler", &e))?;
                    Ok(valor != 0)
                }
                Mecanismo::AnimacaoJanelas => {
                    let mut info = ANIMATIONINFO {
                        cbSize: size_of::<ANIMATIONINFO>() as u32,
                        iMinAnimate: 0,
                    };
                    // SAFETY: SPI_GETANIMATION preenche um ANIMATIONINFO com cbSize correto.
                    unsafe {
                        SystemParametersInfoW(
                            SYSTEM_PARAMETERS_INFO_ACTION(SPI_GETANIMATION),
                            info.cbSize,
                            Some(std::ptr::from_mut(&mut info).cast::<c_void>()),
                            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
                        )
                    }
                    .map_err(|e| erro(efeito.nome, "ler", &e))?;
                    Ok(info.iMinAnimate != 0)
                }
                Mecanismo::Registro {
                    chave,
                    valor,
                    desligado,
                    padrao,
                    ..
                } => {
                    let atual =
                        ler_dword(chave, valor).map_err(|e| erro(efeito.nome, "ler", &e))?;
                    Ok(atual.map_or(padrao, |v| v != desligado))
                }
            }
        }

        fn definir(&self, efeito: &Efeito, ligado: bool) -> AppResult<()> {
            let resultado = match efeito.mecanismo {
                Mecanismo::SpiPv { gravar, .. } => {
                    // SAFETY: nessas ações o valor BOOL é passado no próprio ponteiro.
                    unsafe {
                        SystemParametersInfoW(
                            SYSTEM_PARAMETERS_INFO_ACTION(gravar),
                            0,
                            Some(usize::from(ligado) as *mut c_void),
                            gravar_ini(),
                        )
                    }
                }
                Mecanismo::SpiUi { gravar, .. } => {
                    // SAFETY: nessas ações o valor vai em uiParam e pvParam é nulo.
                    unsafe {
                        SystemParametersInfoW(
                            SYSTEM_PARAMETERS_INFO_ACTION(gravar),
                            u32::from(ligado),
                            None,
                            gravar_ini(),
                        )
                    }
                }
                Mecanismo::AnimacaoJanelas => {
                    let mut info = ANIMATIONINFO {
                        cbSize: size_of::<ANIMATIONINFO>() as u32,
                        iMinAnimate: i32::from(ligado),
                    };
                    // SAFETY: ANIMATIONINFO local com cbSize correto.
                    unsafe {
                        SystemParametersInfoW(
                            SYSTEM_PARAMETERS_INFO_ACTION(SPI_SETANIMATION),
                            info.cbSize,
                            Some(std::ptr::from_mut(&mut info).cast::<c_void>()),
                            gravar_ini(),
                        )
                    }
                }
                Mecanismo::Registro {
                    chave,
                    valor,
                    ligado: v_ligado,
                    desligado: v_desligado,
                    ..
                } => gravar_dword(chave, valor, if ligado { v_ligado } else { v_desligado }),
            };
            resultado.map_err(|e| erro(efeito.nome, "alterar", &e))
        }

        fn ler_modo(&self) -> AppResult<Option<u32>> {
            ler_dword(CHAVE_MODO, VALOR_MODO)
                .map_err(|e| erro("modo de efeitos visuais", "ler", &e))
        }

        fn definir_modo(&self, modo: u32) -> AppResult<()> {
            gravar_dword(CHAVE_MODO, VALOR_MODO, modo)
                .map_err(|e| erro("modo de efeitos visuais", "alterar", &e))
        }

        fn notificar(&self) {
            for area in ["ImmersiveColorSet", "WindowMetrics", "TraySettings"] {
                let texto = HSTRING::from(area);
                // SAFETY: a string vive até o fim da chamada; SMTO_ABORTIFHUNG evita travar
                // em janelas que não respondem.
                unsafe {
                    SendMessageTimeoutW(
                        HWND_BROADCAST,
                        WM_SETTINGCHANGE,
                        WPARAM(0),
                        LPARAM(texto.as_ptr() as isize),
                        SMTO_ABORTIFHUNG,
                        1000,
                        None,
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// DTOs e operações
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EfeitoDto {
    pub id: String,
    pub nome: String,
    pub descricao: String,
    pub impacto: Impacto,
    pub manter_ligado: bool,
    pub requer_logoff: bool,
    /// `None` se não foi possível ler o estado atual.
    pub ligado: Option<bool>,
    /// Estado no backup original.
    pub original: Option<bool>,
    pub alterado: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListaEfeitos {
    pub efeitos: Vec<EfeitoDto>,
    /// Se preenchido, nenhuma alteração é permitida (motivo em PT-BR).
    pub bloqueio: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlteracaoEfeito {
    pub id: String,
    pub ligado: bool,
}

pub fn listar_dto(backend: &dyn EffectsBackend, original: Option<&Snapshot>) -> ListaEfeitos {
    let bloqueio = backend.verificar_usuario().err().map(|e| e.to_string());
    let originais: HashMap<&str, bool> = original
        .and_then(|s| s.efeitos.as_ref())
        .map(|e| {
            e.estados
                .iter()
                .map(|x| (x.id.as_str(), x.ligado))
                .collect()
        })
        .unwrap_or_default();

    let efeitos = EFEITOS
        .iter()
        .map(|e| {
            let ligado = if bloqueio.is_some() {
                None
            } else {
                backend.ler(e).ok()
            };
            let original = originais.get(e.id).copied();
            EfeitoDto {
                id: e.id.into(),
                nome: e.nome.into(),
                descricao: e.descricao.into(),
                impacto: e.impacto,
                manter_ligado: e.manter_ligado,
                requer_logoff: e.requer_logoff,
                ligado,
                original,
                alterado: matches!((ligado, original), (Some(a), Some(b)) if a != b),
            }
        })
        .collect();
    ListaEfeitos { efeitos, bloqueio }
}

fn aviso_logoff(efeito: &Efeito) -> Option<String> {
    efeito.requer_logoff.then(|| {
        "Pode ser preciso sair e entrar novamente na conta do Windows para ver a mudança.".into()
    })
}

/// Liga/desliga um único efeito. Exige que o backup original já exista.
pub fn definir(
    backend: &dyn EffectsBackend,
    servicos: &dyn ServiceBackend,
    log: &Logger,
    store: &SnapshotStore,
    id: &str,
    ligado: bool,
) -> AppResult<Option<String>> {
    let efeito = buscar(id)?;
    backend.verificar_usuario()?;
    garantir_backup_de_efeitos(servicos, log, store)?;

    backend.definir(efeito, ligado)?;
    // Sem isso, a tela "Opções de desempenho" do Windows pode reaplicar um modo automático.
    let _ = backend.definir_modo(MODO_PERSONALIZADO);
    backend.notificar();
    log.info(format!(
        "Efeito visual {} {}",
        efeito.id,
        if ligado { "ligado" } else { "desligado" }
    ));
    Ok(aviso_logoff(efeito))
}

/// Garante que existe um backup com o estado dos efeitos antes de mudar o primeiro.
///
/// Normalmente é o original. Se o original foi gravado por uma versão sem efeitos
/// visuais, cria um backup automático para não ficar sem caminho de volta.
fn garantir_backup_de_efeitos(
    servicos: &dyn ServiceBackend,
    log: &Logger,
    store: &SnapshotStore,
) -> AppResult<()> {
    store.garantir_original(servicos)?;
    let original_tem_efeitos = store
        .original()?
        .is_some_and(|o| o.efeitos.as_ref().is_some_and(|e| !e.estados.is_empty()));
    if original_tem_efeitos {
        return Ok(());
    }
    let ja_existe = store.listar()?.iter().any(|s| s.total_efeitos > 0);
    if !ja_existe {
        let info = criar_backup_com_efeitos(
            servicos,
            store,
            "Estado dos efeitos visuais antes da primeira alteração",
        )?;
        log.info(format!("Snapshot automático {} (efeitos visuais)", info.id));
    }
    Ok(())
}

/// Cria um backup automático e confirma que os efeitos foram salvos nele.
fn criar_backup_com_efeitos(
    servicos: &dyn ServiceBackend,
    store: &SnapshotStore,
    motivo: &str,
) -> AppResult<crate::snapshot::SnapshotInfo> {
    let info = store.criar(servicos, SnapshotKind::Automatico, motivo)?;
    if info.total_efeitos == 0 {
        return Err(AppError::Outro(
            "O backup não conseguiu salvar o estado dos efeitos visuais; nada foi alterado.".into(),
        ));
    }
    Ok(info)
}

/// Aplica várias alterações de efeitos. Cria um backup automático antes, a menos que
/// `snapshot_existente` já tenha sido criado na mesma operação (ex.: restauração).
pub fn aplicar_lote(
    backend: &dyn EffectsBackend,
    servicos: &dyn ServiceBackend,
    log: &Logger,
    store: &SnapshotStore,
    motivo: &str,
    alteracoes: &[AlteracaoEfeito],
    snapshot_existente: Option<String>,
) -> AppResult<ApplyReport> {
    backend.verificar_usuario()?;
    store.garantir_original(servicos)?;

    let mut relatorio = ApplyReport::default();
    let mut planejados = Vec::new();
    for alteracao in alteracoes {
        let efeito = buscar(&alteracao.id)?;
        match backend.ler(efeito) {
            Ok(atual) if atual == alteracao.ligado => {
                relatorio
                    .ignorados
                    .push(item(efeito, "Já está no estado desejado.".into(), None))
            }
            _ => planejados.push((efeito, alteracao.ligado)),
        }
    }

    if planejados.is_empty() {
        relatorio.snapshot_id = snapshot_existente;
        return Ok(relatorio);
    }

    let snapshot_id = match snapshot_existente {
        Some(id) => id,
        None => {
            let info = criar_backup_com_efeitos(servicos, store, motivo).inspect_err(|erro| {
                log.erro(format!(
                    "Lote de efeitos cancelado ({motivo}): backup automático falhou: {erro}"
                ));
            })?;
            log.info(format!("Snapshot automático {} ({motivo})", info.id));
            info.id
        }
    };

    for (efeito, ligado) in planejados {
        let acao = if ligado { "Ligado" } else { "Desligado" };
        match backend.definir(efeito, ligado) {
            Ok(()) => {
                log.info(format!(
                    "Efeito visual {} {}",
                    efeito.id,
                    acao.to_lowercase()
                ));
                relatorio
                    .sucessos
                    .push(item(efeito, acao.into(), aviso_logoff(efeito)));
            }
            Err(erro) => {
                log.erro(format!("Efeito visual {}: {erro}", efeito.id));
                relatorio.falhas.push(item(efeito, erro.to_string(), None));
            }
        }
    }

    if !relatorio.sucessos.is_empty() {
        let _ = backend.definir_modo(MODO_PERSONALIZADO);
        backend.notificar();
    }
    relatorio.snapshot_id = Some(snapshot_id);
    Ok(relatorio)
}

/// Alterações para voltar os efeitos ao estado salvo, e o modo original do Windows.
pub fn plano_de_restauracao(salvos: &EfeitosSalvos) -> Vec<AlteracaoEfeito> {
    salvos
        .estados
        .iter()
        .filter(|e| buscar(&e.id).is_ok())
        .map(|e| AlteracaoEfeito {
            id: e.id.clone(),
            ligado: e.ligado,
        })
        .collect()
}

/// Restaura os efeitos de um snapshot (inclusive o modo `VisualFXSetting` original).
pub fn restaurar(
    backend: &dyn EffectsBackend,
    servicos: &dyn ServiceBackend,
    log: &Logger,
    store: &SnapshotStore,
    salvos: &EfeitosSalvos,
    motivo: &str,
    snapshot_existente: Option<String>,
) -> AppResult<ApplyReport> {
    let relatorio = aplicar_lote(
        backend,
        servicos,
        log,
        store,
        motivo,
        &plano_de_restauracao(salvos),
        snapshot_existente,
    )?;
    if let Some(modo) = salvos.modo_windows {
        let _ = backend.definir_modo(modo);
    }
    Ok(relatorio)
}

fn item(efeito: &Efeito, detalhe: String, aviso: Option<String>) -> ItemRelatorio {
    ItemRelatorio {
        name: efeito.id.into(),
        display_name: efeito.nome.into(),
        detalhe,
        aviso,
    }
}

// ---------------------------------------------------------------------------
// Mock para testes
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod mock {
    use std::collections::{HashMap, HashSet};
    use std::sync::Mutex;

    use super::{EFEITOS, Efeito, EffectsBackend};
    use crate::error::{AppError, AppResult};

    #[derive(Debug, Default)]
    struct Estado {
        valores: HashMap<&'static str, bool>,
        modo: Option<u32>,
        outro_usuario: bool,
        falhar: HashSet<&'static str>,
        gravacoes: Vec<(String, bool)>,
        notificacoes: usize,
    }

    #[derive(Debug, Default)]
    pub struct MockEfeitos {
        estado: Mutex<Estado>,
    }

    impl MockEfeitos {
        /// Todos os efeitos ligados, modo "deixar o Windows escolher" (0).
        pub fn new() -> Self {
            let mock = Self::default();
            {
                let mut e = mock.estado.lock().unwrap();
                e.valores = EFEITOS.iter().map(|x| (x.id, true)).collect();
                e.modo = Some(0);
            }
            mock
        }
        pub fn outro_usuario(self) -> Self {
            self.estado.lock().unwrap().outro_usuario = true;
            self
        }
        pub fn falhar(self, id: &'static str) -> Self {
            self.estado.lock().unwrap().falhar.insert(id);
            self
        }
        pub fn valor(&self, id: &str) -> bool {
            self.estado.lock().unwrap().valores[id]
        }
        pub fn modo(&self) -> Option<u32> {
            self.estado.lock().unwrap().modo
        }
        pub fn gravacoes(&self) -> Vec<(String, bool)> {
            self.estado.lock().unwrap().gravacoes.clone()
        }
        pub fn notificacoes(&self) -> usize {
            self.estado.lock().unwrap().notificacoes
        }
    }

    impl EffectsBackend for MockEfeitos {
        fn verificar_usuario(&self) -> AppResult<()> {
            if self.estado.lock().unwrap().outro_usuario {
                Err(AppError::Outro("outra conta".into()))
            } else {
                Ok(())
            }
        }
        fn ler(&self, efeito: &Efeito) -> AppResult<bool> {
            Ok(self.estado.lock().unwrap().valores[efeito.id])
        }
        fn definir(&self, efeito: &Efeito, ligado: bool) -> AppResult<()> {
            let mut e = self.estado.lock().unwrap();
            if e.falhar.contains(efeito.id) {
                return Err(AppError::Outro("falha simulada".into()));
            }
            e.valores.insert(efeito.id, ligado);
            e.gravacoes.push((efeito.id.to_string(), ligado));
            Ok(())
        }
        fn ler_modo(&self) -> AppResult<Option<u32>> {
            Ok(self.estado.lock().unwrap().modo)
        }
        fn definir_modo(&self, modo: u32) -> AppResult<()> {
            self.estado.lock().unwrap().modo = Some(modo);
            Ok(())
        }
        fn notificar(&self) {
            self.estado.lock().unwrap().notificacoes += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::mock::MockEfeitos;
    use super::*;
    use crate::scm::StartType;
    use crate::scm::mock::MockBackend;

    struct Ambiente {
        _dir: tempfile::TempDir,
        efeitos: Arc<MockEfeitos>,
        servicos: MockBackend,
        store: SnapshotStore,
    }

    fn ambiente(efeitos: MockEfeitos) -> Ambiente {
        let dir = tempfile::tempdir().unwrap();
        let efeitos = Arc::new(efeitos);
        let servicos = MockBackend::com(MockBackend::new(), "Fax", StartType::Manual, false);
        let store = SnapshotStore::new(dir.path().join("snapshots")).com_efeitos(efeitos.clone());
        Ambiente {
            _dir: dir,
            efeitos,
            servicos,
            store,
        }
    }

    fn alteracao(id: &str, ligado: bool) -> AlteracaoEfeito {
        AlteracaoEfeito {
            id: id.into(),
            ligado,
        }
    }

    #[test]
    fn catalogo_de_efeitos_e_consistente() {
        let mut ids = std::collections::HashSet::new();
        for e in EFEITOS {
            assert!(ids.insert(e.id), "id duplicado: {}", e.id);
            assert!(!e.nome.is_empty() && !e.descricao.is_empty(), "{}", e.id);
            assert!(
                e.id.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "id inválido: {}",
                e.id
            );
        }
        // Itens que tornam o Windows difícil de usar nunca são recomendados para desligar.
        for id in ["suavizar_fontes", "miniaturas", "arrastar_conteudo"] {
            assert!(buscar(id).unwrap().manter_ligado, "{id}");
        }
        assert!(buscar("nao_existe").is_err());
    }

    #[test]
    fn lote_cria_backup_com_efeitos_antes_de_alterar() {
        let Ambiente {
            efeitos,
            servicos,
            store,
            ..
        } = ambiente(MockEfeitos::new());
        let log = Logger::desativado();

        let r = aplicar_lote(
            efeitos.as_ref(),
            &servicos,
            &log,
            &store,
            "teste",
            &[
                alteracao("transparencia", false),
                alteracao("menus_fade", true),
            ],
            None,
        )
        .unwrap();

        assert_eq!(r.sucessos.len(), 1);
        assert_eq!(r.ignorados.len(), 1, "menus_fade já estava ligado");
        assert!(!efeitos.valor("transparencia"));
        assert_eq!(efeitos.modo(), Some(MODO_PERSONALIZADO));
        assert_eq!(efeitos.notificacoes(), 1);

        // O backup automático guardou o estado ANTES da mudança.
        let backup = store.carregar(r.snapshot_id.as_deref().unwrap()).unwrap();
        let salvos = backup.efeitos.unwrap();
        assert!(
            salvos
                .estados
                .iter()
                .any(|e| e.id == "transparencia" && e.ligado)
        );
        assert_eq!(salvos.modo_windows, Some(0));

        // E o original também existe, com os efeitos.
        assert!(store.original().unwrap().unwrap().efeitos.is_some());
    }

    #[test]
    fn lote_sem_mudancas_nao_cria_backup() {
        let Ambiente {
            efeitos,
            servicos,
            store,
            ..
        } = ambiente(MockEfeitos::new());
        let r = aplicar_lote(
            efeitos.as_ref(),
            &servicos,
            &Logger::desativado(),
            &store,
            "teste",
            &[alteracao("transparencia", true)],
            None,
        )
        .unwrap();
        assert!(r.snapshot_id.is_none());
        assert!(efeitos.gravacoes().is_empty());
        assert_eq!(store.listar().unwrap().len(), 1, "só o original");
    }

    #[test]
    fn outra_conta_bloqueia_qualquer_alteracao() {
        let Ambiente {
            efeitos,
            servicos,
            store,
            ..
        } = ambiente(MockEfeitos::new().outro_usuario());
        let log = Logger::desativado();
        assert!(
            definir(
                efeitos.as_ref(),
                &servicos,
                &log,
                &store,
                "transparencia",
                false
            )
            .is_err()
        );
        assert!(
            aplicar_lote(
                efeitos.as_ref(),
                &servicos,
                &log,
                &store,
                "t",
                &[alteracao("transparencia", false)],
                None
            )
            .is_err()
        );
        assert!(efeitos.gravacoes().is_empty());

        let lista = listar_dto(efeitos.as_ref(), None);
        assert!(lista.bloqueio.is_some());
        assert!(lista.efeitos.iter().all(|e| e.ligado.is_none()));
    }

    #[test]
    fn falha_em_um_efeito_nao_interrompe_o_lote() {
        let Ambiente {
            efeitos,
            servicos,
            store,
            ..
        } = ambiente(MockEfeitos::new().falhar("aero_peek"));
        let r = aplicar_lote(
            efeitos.as_ref(),
            &servicos,
            &Logger::desativado(),
            &store,
            "t",
            &[
                alteracao("aero_peek", false),
                alteracao("sombra_janelas", false),
            ],
            None,
        )
        .unwrap();
        assert_eq!(r.falhas.len(), 1);
        assert_eq!(r.sucessos.len(), 1);
        assert!(!efeitos.valor("sombra_janelas"));
    }

    #[test]
    fn id_desconhecido_e_rejeitado_sem_alterar_nada() {
        let Ambiente {
            efeitos,
            servicos,
            store,
            ..
        } = ambiente(MockEfeitos::new());
        let r = aplicar_lote(
            efeitos.as_ref(),
            &servicos,
            &Logger::desativado(),
            &store,
            "t",
            &[
                alteracao("sombra_janelas", false),
                alteracao("hackear", false),
            ],
            None,
        );
        assert!(r.is_err());
        assert!(efeitos.gravacoes().is_empty());
    }

    #[test]
    fn restaurar_volta_ao_original_e_ao_modo_do_windows() {
        let Ambiente {
            efeitos,
            servicos,
            store,
            ..
        } = ambiente(MockEfeitos::new());
        let log = Logger::desativado();
        definir(
            efeitos.as_ref(),
            &servicos,
            &log,
            &store,
            "transparencia",
            false,
        )
        .unwrap();
        definir(
            efeitos.as_ref(),
            &servicos,
            &log,
            &store,
            "animar_janelas",
            false,
        )
        .unwrap();
        assert_eq!(efeitos.modo(), Some(MODO_PERSONALIZADO));

        let original = store.original().unwrap().unwrap();
        let lista = listar_dto(efeitos.as_ref(), Some(&original));
        assert_eq!(lista.efeitos.iter().filter(|e| e.alterado).count(), 2);

        let r = restaurar(
            efeitos.as_ref(),
            &servicos,
            &log,
            &store,
            original.efeitos.as_ref().unwrap(),
            "restaurar",
            None,
        )
        .unwrap();
        assert_eq!(r.sucessos.len(), 2);
        assert!(efeitos.valor("transparencia"));
        assert!(efeitos.valor("animar_janelas"));
        assert_eq!(
            efeitos.modo(),
            Some(0),
            "modo original do Windows restaurado"
        );
    }

    #[test]
    fn definir_avisa_quando_precisa_sair_e_entrar() {
        let Ambiente {
            efeitos,
            servicos,
            store,
            ..
        } = ambiente(MockEfeitos::new());
        let log = Logger::desativado();
        let aviso = definir(
            efeitos.as_ref(),
            &servicos,
            &log,
            &store,
            "transparencia",
            false,
        )
        .unwrap();
        assert!(aviso.is_some());
        let aviso = definir(
            efeitos.as_ref(),
            &servicos,
            &log,
            &store,
            "menus_fade",
            false,
        )
        .unwrap();
        assert!(aviso.is_none());
    }

    /// Somente leitura no Windows real: valida o código `unsafe` sem alterar nada.
    #[test]
    fn windows_real_le_todos_os_efeitos_sem_alterar_nada() {
        let backend = WindowsEfeitos;
        for efeito in EFEITOS {
            let r = backend.ler(efeito);
            assert!(r.is_ok(), "{}: {:?}", efeito.id, r);
            println!("{:<22} {:?}", efeito.id, r);
        }
        assert!(backend.ler_modo().is_ok());
        // Em terminal comum o processo é do mesmo usuário do Explorer; em CI pode não
        // haver área de trabalho, então só garante que não entra em pânico.
        println!("verificar_usuario: {:?}", backend.verificar_usuario());
    }
}
