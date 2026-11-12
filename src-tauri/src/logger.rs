//! Log de operações em `%APPDATA%/WinTune/logs/wintune-AAAA-MM-DD.log`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Local;

/// Logger simples, um arquivo por dia. Falhas de escrita nunca derrubam o app.
#[derive(Debug)]
pub struct Logger {
    dir: Option<PathBuf>,
    trava: Mutex<()>,
}

impl Logger {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: Some(dir.into()),
            trava: Mutex::new(()),
        }
    }

    /// Logger que descarta tudo (usado em testes).
    pub fn desativado() -> Self {
        Self {
            dir: None,
            trava: Mutex::new(()),
        }
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    pub fn info(&self, mensagem: impl AsRef<str>) {
        self.escrever("INFO", mensagem.as_ref());
    }

    pub fn aviso(&self, mensagem: impl AsRef<str>) {
        self.escrever("AVISO", mensagem.as_ref());
    }

    pub fn erro(&self, mensagem: impl AsRef<str>) {
        self.escrever("ERRO", mensagem.as_ref());
    }

    fn escrever(&self, nivel: &str, mensagem: &str) {
        let Some(dir) = &self.dir else { return };
        let _guarda = self.trava.lock().unwrap_or_else(|e| e.into_inner());

        let agora = Local::now();
        let arquivo = dir.join(format!("wintune-{}.log", agora.format("%Y-%m-%d")));
        // Uma entrada por linha, mesmo que a mensagem tenha quebras.
        let linha = format!(
            "{} [{nivel}] {}\n",
            agora.format("%Y-%m-%d %H:%M:%S"),
            mensagem.replace(['\r', '\n'], " ")
        );

        let resultado = fs::create_dir_all(dir).and_then(|()| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&arquivo)
                .and_then(|mut f| f.write_all(linha.as_bytes()))
        });
        if let Err(erro) = resultado {
            eprintln!("WinTune: não foi possível gravar o log: {erro}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grava_uma_linha_por_operacao() {
        let dir = tempfile::tempdir().unwrap();
        let logger = Logger::new(dir.path().join("logs"));
        logger.info("primeira operação");
        logger.erro("falha\ncom quebra de linha");

        let arquivos: Vec<_> = fs::read_dir(dir.path().join("logs"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(arquivos.len(), 1);

        let conteudo = fs::read_to_string(&arquivos[0]).unwrap();
        let linhas: Vec<_> = conteudo.lines().collect();
        assert_eq!(linhas.len(), 2);
        assert!(linhas[0].contains("[INFO] primeira operação"));
        assert!(linhas[1].contains("[ERRO] falha com quebra de linha"));
    }

    #[test]
    fn logger_desativado_nao_cria_arquivos() {
        let logger = Logger::desativado();
        logger.info("ignorado");
        assert!(logger.dir().is_none());
    }
}
