# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Projeto

`dev-cli` é uma CLI em Rust (edition 2024 — exige toolchain recente —, binário
único, sem workspace) — um canivete suíço para tarefas de desenvolvimento.

É um **projeto de aprendizado**: o objetivo é ganhar fluência em Rust
construindo uma ferramenta real, em iterações pequenas. Por isso o código é
**comentado de forma didática** — ver Convenções. Consulte
`docs/dev-cli-mentorship.md` para filosofia e roadmap. O mentor atua como Staff
Engineer: explica o "porquê", evita entregar a solução completa e provoca o
raciocínio.

## Comandos

```bash
cargo build                 # compila (debug)
cargo run -- version        # executa um subcomando
cargo run -- logs stats     # estatísticas de logs de todos os containers
cargo run -- ai stats opencode   # dashboard de tokens/custo do OpenCode (heatmap + modelos)
cargo run -- ai stats claude     # horas trabalhadas + custo estimado do Claude Code (mês atual)
cargo build --release       # binário em target/release/dev-cli
cargo test                  # roda a suíte (28 testes em src/logs.rs, src/ai/render.rs, src/ai/precos.rs e src/ai/cambio.rs)
cargo test contar           # roda testes cujo nome casa com "contar"
cargo clippy                # rode antes de dar por pronto (ver abaixo)
```

**Antes de considerar uma mudança pronta, rode `cargo clippy` — não só
`cargo test`.** O usuário usa clippy no editor (LazyVim/rust-analyzer) e o CI
falha em warnings, então código com warning de clippy conta como incompleto.

CI (`.github/workflows/rust.yml`) roda `cargo build`, `cargo test` e
`cargo clippy -- -D warnings` em push/PR na `main`. Não há rustfmt nem
pre-commit configurados — siga o estilo do arquivo que edita.

## Arquitetura

Dispatch por `execute()` que retorna `Result<String, Box<dyn Error>>`:

- `src/main.rs` — entry point. `Cli::parse()`, chama `command.execute()`,
  imprime o `Ok(String)` em stdout ou o erro em stderr com `exit(1)`. Erro sai
  via `Display` (`{error}`); para contexto de debug, trocar para `{error:?}`.
- `src/cli.rs` — `struct Cli` (wrapper do `#[command(subcommand)]`), enum
  `Commands`, e `VersionArgs` com seu `execute()`.
- `src/logs.rs` — subcomando `logs stats`. Padrão central: separar o **núcleo
  puro** (`fn contar(&str) -> Contagens`, sem IO, 100% testável com strings
  inline) da **casca de IO** (descoberta de arquivos via `read_dir`, leitura, e
  render colorido com `owo-colors`).

## Convenções

- **Português (pt-br)** em struct, função e variável. Inglês só em
  crates/traits externas e em nomes de subcommand da CLI pública.
- **Comentários didáticos são bem-vindos.** Por ser projeto de aprendizado,
  comente métodos e funções explicando o que o trecho faz e o conceito de Rust
  por trás (ownership, iteradores, `entry` API, `Result`, etc.). Foque no
  conceito/"porquê", sem parafrasear o óbvio.
- **Sem `unwrap()` em produção** — apenas sob `#[cfg(test)]`.
- **Código clippy-clean.** Preferir os idioms que o clippy pede; em especial,
  usar **let chains** da edition 2024 (`if let Some(x) = y && cond { ... }`) em
  vez de `if` aninhados (lint `collapsible_if`).
- **`Box<dyn Error>` por enquanto.** Migração para `enum CliError + thiserror`
  está prevista (Sprint 3 do roadmap).
- **Novo subcomando:** adicionar `*Args` com `execute()` e a variante no enum
  `Commands` + braço no `match` (em `src/cli.rs`); se a lógica crescer, extrair
  para módulo próprio. Manter a parte pura (cálculo) separada da parte de efeito
  (IO) — ver `src/logs.rs`.
- **Testes não dependem de `dados/`** (fixtures de log; gitignored, some em
  clones) — use strings inline. `dados/` e `/target` estão no `.gitignore`.

## Git

- Conventional Commits em pt-br: `<tipo>(<escopo>): <resumo no imperativo>`.
  Tipos: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `ci`.
- **Não commitar, push ou abrir PR** sem pedido explícito do usuário.
- Sem `-i` interativo, sem force-push, sem pular hooks, sem commits vazios.
- Repositório: `github.com/jarede/dev-cli` (branch `main`). Usar `gh` CLI.
