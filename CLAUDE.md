# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Projeto

`dev-cli` é uma CLI em Rust (edition 2024 — exige toolchain recente) — um
canivete suíço para tarefas de desenvolvimento. É um workspace Cargo com
`crates/nucleo` (lib: parse, métricas, coleta docker/SSH, SQLite) e
`crates/cli` (bin `dev-cli`: clap + dashboard TUI em ratatui) e
`crates/servidor` (bin `dev-server`: axum, coleta contínua + API JSON;
deploy systemd em `deploy/`).

É um **projeto de aprendizado**: o objetivo é ganhar fluência em Rust
construindo uma ferramenta real, em iterações pequenas. Por isso o código é
**comentado de forma didática** — ver Convenções. Consulte
`docs/dev-cli-mentorship.md` para filosofia e roadmap. O mentor atua como Staff
Engineer: explica o "porquê", evita entregar a solução completa e provoca o
raciocínio.

## Comandos

```bash
cargo build                 # compila (debug)
cargo run -p dev-cli -- version        # executa um subcomando
cargo run -p dev-cli -- logs stats     # estatísticas de logs de todos os containers
cargo run -p dev-cli -- logs dashboard              # dashboard TUI ao vivo (docker local)
cargo run -p dev-cli -- logs dashboard --ssh user@host   # idem, coletando via SSH
cargo run -p dev-cli -- ai stats opencode   # dashboard de tokens/custo do OpenCode (heatmap + modelos)
cargo run -p dev-cli -- ai stats claude     # horas trabalhadas + custo estimado do Claude Code (mês atual)
cargo run -p servidor -- --db /tmp/dev.db   # dev-server: coleta + API JSON em 127.0.0.1:8787
cargo build --release       # binário em target/release/dev-cli
cargo test --workspace      # roda a suíte inteira (crates/nucleo + crates/cli)
cargo test contar           # roda testes cujo nome casa com "contar"
cargo clippy --workspace    # clippy no workspace inteiro (rode antes de dar por pronto — ver abaixo)
```

**Antes de considerar uma mudança pronta, rode `cargo clippy --workspace` —
não só `cargo test`.** O usuário usa clippy no editor (LazyVim/rust-analyzer)
e o CI falha em warnings, então código com warning de clippy conta como
incompleto.

CI (`.github/workflows/rust.yml`) roda `cargo build`, `cargo test` e
`cargo clippy -- -D warnings` em push/PR na `main`. Não há rustfmt nem
pre-commit configurados — siga o estilo do arquivo que edita.

## Arquitetura

Workspace com dois crates. Dispatch por `execute()` que retorna
`Result<String, Box<dyn Error>>`:

- **`crates/nucleo`** — lib sem NENHUMA dependência de terminal
  (clap/ratatui/cores), consumida por `crates/cli` e (Fase 2) pelo futuro
  `crates/servidor`:
  - `core.rs` — parse puro de logs (`fn contar(&str) -> Contagens`, sem IO,
    100% testável com strings inline).
  - `metricas.rs` — núcleo puro de métricas: p95, `Severidade`,
    `ResumoContainer`.
  - `db.rs` — casca de IO: schema SQLite, persistência de contagens/linhas/
    requests, `resumo_janela`.
  - `config.rs` — `Config` via TOML + variáveis `DEV_CLI_*`.
  - `executor.rs` — executa `docker` local ou via SSH (enum `Executor`).
  - `coletor.rs` — ciclo de coleta reutilizável e thread coletora (canais
    mpsc), usado pelo dashboard ao vivo.
- **`crates/cli`** — bin `dev-cli`:
  - `main.rs` — entry point. `Cli::parse()`, chama `command.execute()`,
    imprime o `Ok(String)` em stdout ou o erro em stderr com `exit(1)`. Erro
    sai via `Display` (`{error}`); para contexto de debug, trocar para
    `{error:?}`.
  - `cli.rs` — `struct Cli` (wrapper do `#[command(subcommand)]`), enum
    `Commands`, e `VersionArgs` com seu `execute()`.
  - `logs/` — subcomandos `logs stats|containers|remote|dashboard`; a parte
    de apresentação (`render.rs`, cores via `owo-colors`) fica aqui, fora do
    nucleo.
  - `tui.rs` + `screens/` — telas ratatui empilhadas (`Screen` trait); o
    dashboard (`screens/dashboard.rs`) é a tela inicial de `logs dashboard` e
    reage à coleta ao vivo via canal mpsc.
- **`crates/servidor`** — bin `dev-server` (Fase 2): a mesma thread coletora
  do dashboard (`nucleo::coletor`) + API JSON em axum lendo o mesmo SQLite
  (WAL; conexão da API atrás de `Arc<Mutex<...>>`). Rotas em `api.rs`
  (`/api/saude`, `/api/containers`, `/api/containers/{nome}/linhas`,
  `/api/alertas`); `main.rs` faz config -> coletor -> serve com graceful
  shutdown. `deploy/` tem a unit systemd e o `instalar.sh` (RHEL, usuário
  `dev-cli` no grupo docker).

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
  `Commands` + braço no `match` (em `crates/cli/src/cli.rs`); se a lógica
  crescer, extrair para módulo próprio. Manter a parte pura (cálculo) no
  `crates/nucleo` separada da casca de IO — ver `crates/nucleo/src/core.rs`.
- **Testes não dependem de `dados/`** (fixtures de log; gitignored, some em
  clones) — use strings inline. `dados/` e `/target` estão no `.gitignore`.

## Git

- Conventional Commits em pt-br: `<tipo>(<escopo>): <resumo no imperativo>`.
  Tipos: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `ci`.
- **Não commitar, push ou abrir PR** sem pedido explícito do usuário.
- Sem `-i` interativo, sem force-push, sem pular hooks, sem commits vazios.
- Repositório: `github.com/jarede/dev-cli` (branch `main`). Usar `gh` CLI.
