# Política de Segurança

O WinTune roda como administrador e altera serviços do Windows, então levamos segurança a
sério.

## Versões suportadas

Apenas a versão mais recente publicada em [Releases](../../releases) recebe correções.

## Como relatar uma vulnerabilidade

**Não abra uma issue pública.** Use o
[relato privado de vulnerabilidades do GitHub](../../security/advisories/new)
(aba *Security* → *Report a vulnerability*).

Inclua, se possível:

- versão do WinTune e do Windows;
- passos para reproduzir;
- impacto esperado (ex.: contornar a lista de bloqueio, executar código, corromper
  backups).

Você deve receber uma resposta em até 7 dias. Depois da correção, o problema será
divulgado com crédito a quem relatou, se desejar.

## O que conta como vulnerabilidade

- Contornar o `guard` e alterar um serviço bloqueado.
- Fazer o app alterar serviços sem criar backup antes de um lote.
- Execução de código ou escalonamento de privilégio a partir do app ou do instalador.
- Leitura/escrita de arquivos fora de `%APPDATA%\WinTune` via comandos do app
  (ex.: *path traversal* no id de um backup).

## O que não conta

- Um serviço do catálogo que, desativado, quebra um recurso descrito na própria
  descrição dele. Isso é um bug de catálogo: abra uma issue normal.
- Problemas que exigem que o atacante já seja administrador da máquina.
