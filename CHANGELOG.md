# Changelog

Todas as mudanças relevantes deste projeto são registradas aqui.

O formato segue o [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e o projeto
usa [Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

### Adicionado

- Tela **Efeitos visuais**: 18 efeitos do Windows (animações, sombras, transparência,
  miniaturas, suavização de fontes etc.) com liga/desliga individual e os atalhos
  "Melhor desempenho", "Desligar tudo" e "Restaurar original".
- Backups passam a guardar também o estado dos efeitos visuais, e restaurar um backup
  restaura os dois.

## [0.1.0] - 2026-11-12

### Adicionado

- Listagem de todos os serviços do Windows com status, tipo de inicialização, categoria,
  risco e descrição em português.
- Ligar/desligar e alterar o tipo de inicialização de cada serviço.
- Lista de bloqueio de serviços críticos aplicada no backend.
- Catálogo curado com 45 serviços.
- Perfis Leve, Gamer, Máximo e Personalizado, com prévia das mudanças.
- Backup original na primeira execução e backup automático antes de cada lote.
- Restauração e exclusão de backups.
- Criação de ponto de restauração do Windows, respeitando o limite de 24 horas.
- Dashboard com totais e uso de RAM.
- Log de todas as operações em `%APPDATA%\WinTune\logs`.
- Instalador NSIS que exige administrador.

[Não lançado]: ../../compare/v0.1.0...HEAD
[0.1.0]: ../../releases/tag/v0.1.0
