## O que muda

<!-- Descreva a mudança e o motivo. Se resolve uma issue, escreva "Resolve #123". -->

## Como testei

<!-- Comandos rodados, versão do Windows, capturas de tela se mexeu na interface. -->

## Checklist

- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` e `cargo test` passam
- [ ] `pnpm lint` e `pnpm build` passam
- [ ] Nenhum `unwrap()`/`expect()` novo em código de produção
- [ ] Toda escrita em serviços passa pelo `guard` e pelas funções de `ops.rs`
- [ ] Textos da interface em português do Brasil
- [ ] Atualizei o `CHANGELOG.md` (seção "Não lançado"), se for uma mudança visível
- [ ] Se alterei o catálogo: incluí a fonte e as versões do Windows verificadas
