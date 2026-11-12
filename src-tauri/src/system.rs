//! Informações do sistema (RAM, elevação) e ponto de restauração do Windows.

use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use serde::Serialize;
use sysinfo::System;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::error::{AppError, AppResult};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemStats {
    /// Memória RAM total, em bytes.
    pub ram_total: u64,
    /// Memória RAM em uso, em bytes.
    pub ram_usada: u64,
    /// O app está rodando com privilégios de administrador.
    pub elevado: bool,
}

pub fn stats() -> SystemStats {
    // Se a leitura falhar, o painel mostra 0 em vez de derrubar o app.
    let (ram_total, ram_usada) = match System::new() {
        Ok(mut sistema) => {
            sistema.refresh_memory();
            (sistema.total_memory(), sistema.used_memory())
        }
        Err(_) => (0, 0),
    };
    SystemStats {
        ram_total,
        ram_usada,
        elevado: is_elevado(),
    }
}

/// `true` se o processo atual tem um token elevado (administrador).
pub fn is_elevado() -> bool {
    let mut token = HANDLE::default();
    // SAFETY: GetCurrentProcess devolve um pseudo-handle sempre válido; `token` é local.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
        return false;
    }

    let mut elevacao = TOKEN_ELEVATION::default();
    let mut tamanho = 0u32;
    // SAFETY: o buffer é uma struct local do tamanho informado; o token é fechado em seguida.
    let lido = unsafe {
        let resultado = GetTokenInformation(
            token,
            TokenElevation,
            Some(std::ptr::from_mut(&mut elevacao).cast::<c_void>()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut tamanho,
        );
        let _ = CloseHandle(token);
        resultado
    };
    lido.is_ok() && elevacao.TokenIsElevated != 0
}

// ---------------------------------------------------------------------------
// Ponto de restauração do Windows
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RestorePointStatus {
    /// Um novo ponto de restauração foi criado.
    Criado,
    /// Já existe um ponto criado nas últimas 24 h; o Windows não cria outro.
    Recente,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestorePointResult {
    pub status: RestorePointStatus,
    pub mensagem: String,
}

/// O Windows só cria um ponto de restauração a cada 24 h (1440 min) por padrão.
/// Em vez de mexer nesse limite no Registro, o script detecta o caso e avisa.
const SCRIPT_PONTO_RESTAURACAO: &str = r#"
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
try {
    $ultimo = Get-ComputerRestorePoint -ErrorAction SilentlyContinue |
        Sort-Object SequenceNumber | Select-Object -Last 1
    if ($ultimo) {
        $data = [System.Management.ManagementDateTimeConverter]::ToDateTime($ultimo.CreationTime)
        if (((Get-Date) - $data).TotalMinutes -lt 1440) {
            Write-Output ('WINTUNE_RECENTE|' + $data.ToString('dd/MM/yyyy HH:mm'))
            exit 0
        }
    }
    $avisos = @()
    Checkpoint-Computer -Description 'WinTune' -RestorePointType 'MODIFY_SETTINGS' -WarningVariable avisos -WarningAction SilentlyContinue
    if ($avisos.Count -gt 0) {
        Write-Output ('WINTUNE_AVISO|' + (($avisos | ForEach-Object { "$_" }) -join ' '))
        exit 0
    }
    Write-Output 'WINTUNE_OK'
    exit 0
} catch {
    Write-Output ('WINTUNE_ERRO|' + $_.Exception.Message)
    exit 1
}
"#;

/// Base64 padrão (RFC 4648), usado para o `-EncodedCommand` do PowerShell.
fn base64(dados: &[u8]) -> String {
    const ALFABETO: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut saida = String::with_capacity(dados.len().div_ceil(3) * 4);
    for bloco in dados.chunks(3) {
        let b0 = bloco[0];
        let b1 = bloco.get(1).copied().unwrap_or(0);
        let b2 = bloco.get(2).copied().unwrap_or(0);
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        let caractere = |deslocamento: u32| ALFABETO[((n >> deslocamento) & 0x3F) as usize] as char;
        saida.push(caractere(18));
        saida.push(caractere(12));
        saida.push(if bloco.len() > 1 { caractere(6) } else { '=' });
        saida.push(if bloco.len() > 2 { caractere(0) } else { '=' });
    }
    saida
}

fn codificar_comando(script: &str) -> String {
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64(&utf16)
}

/// Caminho absoluto do Windows PowerShell 5.1 (o `Checkpoint-Computer` não existe no
/// PowerShell 7). Caminho absoluto evita executar um `powershell.exe` plantado no PATH.
fn caminho_powershell() -> PathBuf {
    let raiz = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    raiz.join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe")
}

/// Interpreta a saída do script. Separado da execução para ser testável.
fn interpretar_saida(stdout: &str, stderr: &str) -> AppResult<RestorePointResult> {
    let marcador = stdout
        .lines()
        .map(str::trim)
        .rev()
        .find(|linha| linha.starts_with("WINTUNE_"));

    match marcador.map(|linha| linha.split_once('|').unwrap_or((linha, ""))) {
        Some(("WINTUNE_OK", _)) => Ok(RestorePointResult {
            status: RestorePointStatus::Criado,
            mensagem: "Ponto de restauração \"WinTune\" criado com sucesso.".into(),
        }),
        Some(("WINTUNE_RECENTE", data)) => Ok(RestorePointResult {
            status: RestorePointStatus::Recente,
            mensagem: format!(
                "Já existe um ponto de restauração recente (criado em {data}). O Windows só permite criar um novo a cada 24 horas — o ponto existente continua válido."
            ),
        }),
        // O Windows avisa (sem erro) quando recusa a criação pelo limite de 24 h.
        Some(("WINTUNE_AVISO", aviso)) => Ok(RestorePointResult {
            status: RestorePointStatus::Recente,
            mensagem: format!(
                "O Windows não criou um novo ponto de restauração, provavelmente porque já existe um das últimas 24 horas. Detalhe: {aviso}"
            ),
        }),
        Some(("WINTUNE_ERRO", detalhe)) => Err(AppError::Outro(format!(
            "Não foi possível criar o ponto de restauração. Verifique se o WinTune está rodando como administrador e se a Proteção do Sistema está ativada na unidade C: (Painel de Controle > Sistema > Proteção do Sistema). Detalhe: {detalhe}"
        ))),
        _ => Err(AppError::Outro(format!(
            "O PowerShell não respondeu como esperado ao criar o ponto de restauração. {}",
            stderr.trim()
        ))),
    }
}

/// Cria um ponto de restauração do Windows via `Checkpoint-Computer`.
pub fn criar_ponto_de_restauracao() -> AppResult<RestorePointResult> {
    let saida = Command::new(caminho_powershell())
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &codificar_comando(SCRIPT_PONTO_RESTAURACAO),
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| AppError::io("executar o PowerShell", e))?;

    interpretar_saida(
        &String::from_utf8_lossy(&saida.stdout),
        &String::from_utf8_lossy(&saida.stderr),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_devolve_memoria_coerente() {
        let s = stats();
        assert!(s.ram_total > 0);
        assert!(s.ram_usada > 0);
        assert!(s.ram_usada <= s.ram_total);
    }

    #[test]
    fn base64_segue_a_rfc_4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn comando_e_codificado_em_utf16le() {
        // "a" em UTF-16LE = 61 00 -> "YQA="
        assert_eq!(codificar_comando("a"), "YQA=");
    }

    #[test]
    fn interpreta_ponto_criado() {
        let r = interpretar_saida("ruído\r\nWINTUNE_OK\r\n", "").unwrap();
        assert_eq!(r.status, RestorePointStatus::Criado);
    }

    #[test]
    fn interpreta_limite_de_24_horas() {
        let r = interpretar_saida("WINTUNE_RECENTE|11/11/2026 09:30\r\n", "").unwrap();
        assert_eq!(r.status, RestorePointStatus::Recente);
        assert!(r.mensagem.contains("11/11/2026 09:30"));
        assert!(r.mensagem.contains("24 horas"));

        let r = interpretar_saida("WINTUNE_AVISO|já foi criado um ponto\r\n", "").unwrap();
        assert_eq!(r.status, RestorePointStatus::Recente);
        assert!(r.mensagem.contains("já foi criado um ponto"));
    }

    #[test]
    fn interpreta_erros() {
        let erro = interpretar_saida("WINTUNE_ERRO|Restauração desabilitada\r\n", "").unwrap_err();
        let texto = erro.to_string();
        assert!(texto.contains("Proteção do Sistema"), "{texto}");
        assert!(texto.contains("Restauração desabilitada"), "{texto}");

        assert!(interpretar_saida("", "powershell quebrou").is_err());
        assert!(interpretar_saida("saída qualquer", "").is_err());
    }

    /// Executa o PowerShell de verdade. Manual: `cargo test -- --ignored --nocapture`.
    /// Em um terminal elevado isso cria (no máximo) um ponto de restauração.
    #[test]
    #[ignore = "executa o Checkpoint-Computer real"]
    fn ponto_de_restauracao_real_nunca_entra_em_panico() {
        match criar_ponto_de_restauracao() {
            Ok(resultado) => println!("OK: {resultado:?}"),
            Err(erro) => println!("ERRO tratado: {erro}"),
        }
    }

    #[test]
    fn powershell_usa_caminho_absoluto() {
        let caminho = caminho_powershell();
        assert!(caminho.is_absolute());
        assert!(caminho.ends_with(r"System32\WindowsPowerShell\v1.0\powershell.exe"));
    }
}
