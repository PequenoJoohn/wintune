//! Erros do WinTune. Toda mensagem é escrita em PT-BR e chega ao front como texto.

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(
        "O serviço \"{0}\" é essencial para o Windows e está bloqueado: o WinTune nunca o altera."
    )]
    ServicoBloqueado(String),

    #[error(
        "O serviço \"{0}\" não pode ser desativado nem parado pelo WinTune. Só é permitido defini-lo como Manual ou Automático."
    )]
    ServicoRestrito(String),

    #[error(
        "\"{0}\" é um serviço por usuário, gerenciado pelo próprio Windows, e não pode ser alterado."
    )]
    ServicoPorUsuario(String),

    #[error("O serviço \"{0}\" não existe neste computador.")]
    ServicoNaoEncontrado(String),

    #[error(
        "Acesso negado ao serviço \"{0}\". Ele é protegido pelo Windows ou o WinTune não está sendo executado como administrador."
    )]
    AcessoNegado(String),

    #[error("Não foi possível {operacao} o serviço \"{servico}\": {detalhe}")]
    Scm {
        servico: String,
        operacao: &'static str,
        detalhe: String,
    },

    #[error("Não foi possível conectar ao Gerenciador de Serviços do Windows: {0}")]
    Gerenciador(String),

    #[error("Erro de arquivo ao {contexto}: {fonte}")]
    Io {
        contexto: String,
        #[source]
        fonte: std::io::Error,
    },

    #[error("Dados inválidos ao {contexto}: {fonte}")]
    Json {
        contexto: String,
        #[source]
        fonte: serde_json::Error,
    },

    #[error("{0}")]
    Invalido(String),

    #[error("{0}")]
    Outro(String),
}

impl AppError {
    pub fn io(contexto: impl Into<String>, fonte: std::io::Error) -> Self {
        Self::Io {
            contexto: contexto.into(),
            fonte,
        }
    }

    pub fn json(contexto: impl Into<String>, fonte: serde_json::Error) -> Self {
        Self::Json {
            contexto: contexto.into(),
            fonte,
        }
    }
}

/// O front recebe apenas a mensagem já formatada em PT-BR.
impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
