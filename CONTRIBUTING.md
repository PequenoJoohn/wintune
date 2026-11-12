# Como contribuir com o WinTune

Obrigado pelo interesse! Contribuições são bem-vindas: correções, novos serviços no
catálogo, melhorias de interface, traduções e documentação.

Ao participar, você concorda em seguir o [Código de Conduta](CODE_OF_CONDUCT.md).

## Antes de começar

- **Bugs e ideias:** abra uma [issue](../../issues/new/choose) usando um dos modelos.
- **Falhas de segurança:** não abra issue pública. Siga a [política de segurança](SECURITY.md).
- Para mudanças grandes, abra uma issue primeiro para combinarmos a abordagem.

## Ambiente de desenvolvimento

Requisitos: Windows 10/11, Rust estável (`x86_64-pc-windows-msvc`), Ferramentas de Build
do Visual Studio (C++), Node.js 20+ e pnpm. Detalhes no [README](README.md#requisitos-para-compilar).

```powershell
pnpm install
pnpm tauri dev      # terminal COMO ADMINISTRADOR
```

## Checklist antes do pull request

Rode tudo abaixo; o CI roda os mesmos comandos e o PR só é aceito com tudo verde.

```powershell
# Backend (dentro de src-tauri/)
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test

# Frontend (na raiz)
pnpm lint
pnpm build
```

Os testes não precisam de administrador e **nunca** alteram serviços reais: toda lógica de
escrita é testada contra o `MockBackend` (`src-tauri/src/scm.rs`).

## Regras do projeto

Estas regras protegem o sistema de quem usa o app. PRs que as quebrem não serão aceitos.

1. **Nada de `unwrap()`/`expect()` em código de produção.** O Clippy bloqueia; erros
   viram mensagens claras em português (`src-tauri/src/error.rs`).
2. **Toda escrita passa pelo `guard`.** Nunca chame o SCM diretamente a partir de um
   comando Tauri; use as funções de `ops.rs`.
3. **Alterações em lote criam backup antes.** Se o backup falhar, nada é alterado.
4. **A lista de bloqueio (`guard.rs`) só cresce.** Remover um serviço dela exige
   justificativa técnica detalhada na issue/PR.
5. **Interface e mensagens em português do Brasil.**
6. **Testes para toda mudança de lógica** no backend.

## Adicionando um serviço ao catálogo

O catálogo fica em [`src-tauri/catalog.json`](src-tauri/catalog.json). Cada entrada:

```json
{
  "name": "NomeDoServico",
  "categoria": "Diversos",
  "risco": "seguro",
  "descricao": "O que o serviço faz. O que para de funcionar se for desativado.",
  "recomendado": "manual"
}
```

- `categoria`: Telemetria, Xbox/Jogos, Impressão, Indexação/Busca, Rede remota,
  Bluetooth, Hyper-V, Mapas/Localização, Fax ou Diversos.
- `risco`: `seguro` (ninguém sente falta), `moderado` (afeta um recurso que algumas
  pessoas usam) ou `avancado` (pode cortar acesso remoto, login ou hardware).
- `recomendado`: `manual` ou `disabled`.
- A descrição deve citar **o que para de funcionar**. Inclua no PR a fonte (documentação
  da Microsoft ou teste próprio) e em quais versões do Windows você verificou.
- `cargo test` valida o JSON e garante que nenhum serviço do catálogo está bloqueado.

## Estilo de commits e PRs

- Commits curtos e descritivos, de preferência no formato
  [Conventional Commits](https://www.conventionalcommits.org/pt-br/) (`feat:`, `fix:`,
  `docs:`, `catalogo:`...).
- Um assunto por PR. Descreva o que mudou, por quê e como você testou.
- Se a mudança afeta a interface, anexe uma captura de tela.

## Licença

Ao enviar uma contribuição, você concorda que ela será distribuída sob a
[licença MIT](LICENSE) do projeto.
