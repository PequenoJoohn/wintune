# WinTune — Otimizador de Serviços do Windows

[![CI](https://github.com/pequenojoohn/wintune/actions/workflows/ci.yml/badge.svg)](https://github.com/pequenojoohn/wintune/actions/workflows/ci.yml)
[![Licença: MIT](https://img.shields.io/badge/licen%C3%A7a-MIT-blue.svg)](LICENSE)
[![Release](https://img.shields.io/github/v/release/pequenojoohn/wintune?include_prereleases&label=vers%C3%A3o)](https://github.com/pequenojoohn/wintune/releases)
![Windows 10/11](https://img.shields.io/badge/Windows-10%20%7C%2011-0078D6)

App desktop open source para Windows 10/11 que desativa serviços desnecessários para liberar recursos.
Cada serviço pode ser ligado ou desligado a qualquer momento, e tudo volta ao estado
original com um clique.

Feito com **Tauri v2** (backend em Rust) e **React + TypeScript + Vite + Tailwind CSS**
(componentes no padrão shadcn/ui).

> ⚠️ **Leia os [avisos de uso](#avisos-de-uso) antes de aplicar qualquer perfil.**

## Funcionalidades

- **Dashboard** — total de serviços, quantos estão em execução, quantos foram desativados
  pelo WinTune, uso de RAM e o botão **Restaurar original**.
- **Serviços** — tabela com busca, filtros por categoria/risco/status, toggle por linha,
  seletor de tipo de inicialização e descrição em português do que cada serviço faz.
- **Perfis** — Leve, Gamer e Máximo, com prévia (diff) antes de aplicar, além de perfis
  personalizados salvos a partir da seleção atual.
- **Backups** — snapshots do estado dos serviços, com restauração e exclusão, e criação de
  ponto de restauração do Windows.

## Download

Baixe o instalador `WinTune_<versão>_x64-setup.exe` na página de
[Releases](https://github.com/pequenojoohn/wintune/releases). O instalador e o app pedem
permissão de administrador (UAC).

> O executável não é assinado digitalmente, então o Windows SmartScreen pode mostrar
> "O Windows protegeu o computador". Clique em **Mais informações → Executar assim mesmo**,
> ou compile você mesmo a partir do código-fonte.

## Requisitos para compilar

- Windows 10 ou 11 (64 bits) com [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/)
  (já vem no Windows 11)
- [Rust](https://rustup.rs/) estável com o toolchain `x86_64-pc-windows-msvc`
- [Ferramentas de Build do Visual Studio](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
  com a carga "Desenvolvimento para desktop com C++"
- [Node.js](https://nodejs.org/) 20+ e [pnpm](https://pnpm.io/)

## Como rodar em desenvolvimento

O executável exige privilégios de administrador (manifest `requireAdministrator`), então
**abra o terminal como administrador** antes de rodar:

```powershell
pnpm install
pnpm tauri dev
```

Em um terminal sem elevação o Windows recusa iniciar o `.exe` (erro 740, "a operação
solicitada requer elevação").

### Testes e verificações

Os testes **não** precisam de administrador: a lógica usa um backend simulado
(`MockBackend`) e o único teste que toca o Windows real apenas lê a lista de serviços.

```powershell
# Backend (dentro de src-tauri/)
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check

# Frontend (na raiz)
pnpm lint
pnpm build        # tsc (strict) + vite build
```

Há um teste manual que executa o `Checkpoint-Computer` de verdade:
`cargo test -- --ignored --nocapture`.

## Como gerar o instalador

```powershell
pnpm tauri build
```

O instalador NSIS é gerado em
`src-tauri/target/release/bundle/nsis/WinTune_<versão>_x64-setup.exe`, e o executável
avulso em `src-tauri/target/release/wintune.exe`. Na primeira vez o Tauri baixa o NSIS
automaticamente.

## Avisos de uso

- **Use por sua conta e risco.** Desativar serviços altera o comportamento do Windows.
  Leia a descrição de cada serviço (passe o mouse sobre o nome) e a prévia de cada perfil.
- O WinTune **sempre** pede elevação (UAC), pois alterar serviços exige administrador.
- Na primeira execução é gravado um backup **original**, que nunca é sobrescrito nem
  excluído. Antes de qualquer alteração em lote (aplicar perfil ou restaurar backup) é
  criado um backup automático. Se o backup não puder ser gravado, nada é alterado.
- Serviços de risco **Avançado** (Área de Trabalho Remota, biometria, teclado virtual)
  nunca entram em perfis: só podem ser desativados manualmente, com confirmação. Não
  desative a Área de Trabalho Remota se você acessa este PC à distância.
- Alguns serviços só param de fato **depois de reiniciar** o computador; o app avisa
  quando isso acontece.
- Atualizações grandes do Windows podem reativar serviços. Basta reaplicar o perfil.
- Recomendado: crie um **ponto de restauração do Windows** (tela Backups) antes do
  primeiro uso. O Windows só cria um ponto a cada 24 horas; se já existir um recente, o
  app avisa e segue usando o existente.

### Serviços bloqueados

Estes serviços são essenciais e o backend recusa qualquer alteração neles, mesmo que a
interface peça:

`RpcSs`, `RpcEptMapper`, `DcomLaunch`, `Winmgmt`, `CryptSvc`, `BFE`, `MpsSvc`, `WinDefend`,
`SecurityHealthService`, `EventLog`, `PlugPlay`, `Power`, `ProfSvc`, `LSM`, `SamSs`,
`Schedule`, `BrokerInfrastructure`, `CoreMessagingRegistrar`, `gpsvc`, `Dhcp`, `Dnscache`,
`nsi`, `AudioSrv`, `AudioEndpointBuilder`, `Themes`, `UserManager`, `TrustedInstaller`,
além de outros acrescentados por precaução (lista completa em
[`src-tauri/src/guard.rs`](src-tauri/src/guard.rs)).

- `wuauserv` (Windows Update) pode ficar em Manual ou Automático, mas **nunca** Desativado.
- Serviços por usuário (sufixo `_xxxxx`) e serviços aos quais o Windows nega acesso
  aparecem com cadeado e não são alterados.

### Perfis

| Perfil | O que desativa |
| --- | --- |
| **Leve** | Somente itens de risco "seguro" (telemetria, Xbox, fax etc.) |
| **Gamer** | Leve + indexação de busca + impressão. Mantém todos os serviços do Xbox |
| **Máximo** | Tudo até o risco "moderado" (Bluetooth, localização, Hyper-V…), com confirmação extra |
| **Personalizado** | Os serviços que você alterou em relação ao estado original |

## Onde ficam os dados

Tudo fica em `%APPDATA%\WinTune\`:

| Caminho | Conteúdo |
| --- | --- |
| `snapshots\original.json` | Estado dos serviços na primeira execução |
| `snapshots\<data-hora>.json` | Backups manuais e automáticos (os 30 automáticos mais recentes) |
| `profiles.json` | Perfis personalizados |
| `logs\wintune-AAAA-MM-DD.log` | Registro de todas as operações |

## Como restaurar manualmente se algo der errado

Se o WinTune não abrir, dá para desfazer tudo pelo **Prompt de Comando como
administrador**. Se o Windows não iniciar normalmente, entre no
[Modo de Segurança](https://support.microsoft.com/windows/92c27cff-db89-8644-1ce4-b3e5e56fe234)
e rode os mesmos comandos, ou use a Restauração do Sistema.

**1. Descubra o tipo de inicialização original** abrindo
`%APPDATA%\WinTune\snapshots\original.json` no Bloco de Notas. Cada serviço tem um
`start_type`:

| `start_type` no arquivo | Comando `sc config` |
| --- | --- |
| `automatic` | `start= auto` |
| `automatic_delayed` | `start= delayed-auto` |
| `manual` | `start= demand` |
| `disabled` | `start= disabled` |

**2. Restaure o serviço** (o espaço depois de `start=` é obrigatório):

```bat
:: Exemplo: reativar o Spooler de Impressão como Automático e iniciá-lo
sc config Spooler start= auto
sc start Spooler

:: Exemplo: Windows Search volta para Automático (atrasado)
sc config WSearch start= delayed-auto
sc start WSearch

:: Conferir como o serviço está agora
sc qc Spooler
sc query Spooler
```

**3. Para restaurar todos de uma vez**, este script PowerShell (como administrador) lê o
backup original e reaplica cada tipo de inicialização:

```powershell
$mapa = @{ automatic = 'auto'; automatic_delayed = 'delayed-auto'; manual = 'demand'; disabled = 'disabled' }
$backup = Get-Content "$env:APPDATA\WinTune\snapshots\original.json" -Raw -Encoding UTF8 | ConvertFrom-Json
foreach ($s in $backup.servicos) {
    if ($s.name -match '_[0-9a-fA-F]{4,8}$') { continue }   # serviços por usuário
    if ($mapa.ContainsKey($s.start_type)) {
        sc.exe config $s.name start= $mapa[$s.start_type] | Out-Null
    }
}
```

Serviços protegidos pelo Windows responderão "Acesso negado"; isso é esperado e pode ser
ignorado. Reinicie o computador ao final.

**4. Último recurso:** `rstrui.exe` abre a Restauração do Sistema do Windows, onde você
pode voltar a um ponto de restauração anterior.

## Arquitetura

```
src/                     Frontend React
  pages/                 Dashboard, Serviços, Perfis, Backups
  components/ui/         Componentes no padrão shadcn/ui
  lib/api.ts             Chamadas tipadas aos comandos Tauri
src-tauri/
  catalog.json           Catálogo curado (categoria, risco, descrição, recomendado)
  wintune.exe.manifest   Manifest com requireAdministrator
  src/
    scm.rs               Wrapper do Service Control Manager + trait ServiceBackend + mock
    guard.rs             Lista de bloqueio de serviços críticos
    catalog.rs           Carga e validação do catálogo embutido
    snapshot.rs          Backups do estado dos serviços
    profiles.rs          Perfis pré-definidos e personalizados
    ops.rs               Motor de operações (guard + log + backup antes de lotes)
    system.rs            RAM, verificação de elevação e ponto de restauração
    commands.rs          Comandos Tauri expostos ao front
    logger.rs / error.rs Log em arquivo e erros em PT-BR
```

Regras que o backend garante, independentemente do front:

1. Toda escrita passa pelo `guard`; serviços bloqueados são recusados antes de tocar o SCM.
2. Só o tipo de inicialização é alterado (`ChangeServiceConfigW` com `SERVICE_NO_CHANGE`
   nos demais campos): binário, conta e dependências do serviço nunca são reescritos.
3. Nenhum `unwrap()`/`expect()` em código de produção (imposto pelo Clippy).
4. Toda operação é registrada em log.

## Contribuindo

Contribuições são bem-vindas, principalmente melhorias no catálogo de serviços. Leia o
[guia de contribuição](CONTRIBUTING.md) e o [código de conduta](CODE_OF_CONDUCT.md).
Falhas de segurança devem ser relatadas de forma privada, conforme a
[política de segurança](SECURITY.md). As mudanças de cada versão ficam no
[CHANGELOG](CHANGELOG.md).

## Licença

Distribuído sob a [licença MIT](LICENSE). O software é fornecido "como está", sem garantia
de qualquer tipo.

O WinTune é um projeto independente, sem afiliação com a Microsoft. Windows é marca
registrada da Microsoft Corporation.
