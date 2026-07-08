# Fase 1 — Workspace + Núcleo de Coleta ao Vivo + Dashboard TUI — Plano de Implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transformar o dev-cli num workspace Cargo com uma lib `nucleo` (coleta docker local/SSH, parse, métricas, SQLite) e um dashboard TUI que mostra, ranqueado por severidade, onde estão os problemas nos containers — com coleta ao vivo em thread.

**Architecture:** Workspace com `crates/nucleo` (lib pura + casca de IO de coleta) e `crates/cli` (bin `dev-cli` com clap + ratatui). Uma thread coletora executa `docker ps`/`docker logs` (local por padrão; SSH opcional), grava em SQLite (WAL) e avisa a TUI por canal mpsc; a TUI relê agregados do banco e redesenha. O mesmo ciclo de coleta será reusado pelo `dev-server` (axum) na Fase 2.

**Tech Stack:** Rust edition 2024, clap 4, ratatui 0.29, crossterm 0.28, rusqlite 0.40 (bundled, WAL), serde + toml, dotenvy, dirs, threads + mpsc da std.

## Global Constraints

- **Português (pt-br)** em nomes de struct, função, variável, enum. Inglês só em crates/traits externas e nomes de subcomandos da CLI pública.
- **Comentários didáticos exaustivos**: todo enum/struct/método novo recebe comentário explicando o conceito de Rust envolvido (ownership, threads, canais, traits etc.) com links `docs:` — siga o estilo dos arquivos existentes. O código deste plano já traz os comentários: **copie-os junto com o código**.
- **Sem `unwrap()`/`expect()` fora de `#[cfg(test)]`**.
- **Clippy-clean**: `cargo clippy --workspace -- -D warnings` deve passar ao fim de CADA task. Use let chains (`if let Some(x) = y && cond {}`) em vez de `if` aninhados.
- **Erros com `Box<dyn std::error::Error>`** (padrão atual do projeto).
- **Conventional Commits em pt-br**: `<tipo>(<escopo>): <resumo no imperativo>`. NUNCA fazer push. Nunca `git commit` com `-i`, sem `--force`, sem pular hooks.
- **Testes não dependem de `dados/`** nem de docker/ssh reais — use strings inline e SQLite em memória (`Connection::open_in_memory()`).
- Rode os comandos sempre a partir da raiz do repositório: `/Users/jarede/projetos/dev-cli`.

---

## Visão do resultado final

```
dev-cli/
├─ Cargo.toml                      # [workspace]
├─ crates/
│  ├─ nucleo/
│  │  ├─ Cargo.toml
│  │  └─ src/
│  │     ├─ lib.rs                 # pub mod core, db, config, executor, metricas, coletor
│  │     ├─ core.rs                # parse puro (movido de src/logs/core.rs)
│  │     ├─ db.rs                  # SQLite (movido de src/logs/db.rs) + requests + resumo_janela
│  │     ├─ config.rs              # Config TOML + env DEV_CLI_*
│  │     ├─ executor.rs            # enum Executor { Local, Ssh } + listar/obter logs
│  │     ├─ metricas.rs            # p95, ResumoContainer, Severidade (puro)
│  │     └─ coletor.rs             # coletar_ciclo + iniciar_coletor (thread + mpsc)
│  └─ cli/
│     ├─ Cargo.toml                # bin dev-cli
│     └─ src/
│        ├─ main.rs, cli.rs, help.rs, tui.rs
│        ├─ ai/                    # (movido sem mudanças)
│        ├─ logs/                  # mod.rs, stats.rs, containers.rs, remote.rs, render.rs, dashboard.rs
│        └─ screens/               # mod.rs, dashboard.rs, app_types.rs, levels.rs, lines.rs, loguru_stats.rs
└─ docs/superpowers/plans/         # este plano
```

Comando final novo: `dev-cli logs dashboard [--config PATH] [--ssh user@host] [--db PATH] [--intervalo SEG]`.

---

### Task 0: Commitar o trabalho em andamento

A árvore tem mudanças não commitadas (refactor do TUI para `src/screens/`). Commitar antes de mover arquivos.

**Files:**
- Nenhum arquivo novo; só git.

- [ ] **Step 0.1: Verificar que a suíte passa**

Run: `cargo test 2>&1 | tail -5`
Expected: `test result: ok.` (28+ testes, 0 failed)

Run: `cargo clippy -- -D warnings 2>&1 | tail -3`
Expected: `Finished` sem erros. Se houver warnings, corrija-os antes de continuar (provavelmente imports não usados nos arquivos modificados).

- [ ] **Step 0.2: Commitar tudo**

```bash
git add src/ && git commit -m "refactor(tui): extrai telas para src/screens/ com pilha de telas"
```

Run: `git status --short`
Expected: saída vazia (árvore limpa; `dados/` e `target/` são gitignored).

---

### Task 1: Reestruturar em workspace (nucleo + cli)

Mover arquivos SEM mudar lógica. Ao final, `cargo build` compila o mesmo programa, agora como workspace.

**Files:**
- Modify: `Cargo.toml` (raiz — vira só `[workspace]`)
- Create: `crates/nucleo/Cargo.toml`, `crates/nucleo/src/lib.rs`
- Create: `crates/cli/Cargo.toml`
- Move: `src/logs/core.rs` → `crates/nucleo/src/core.rs`; `src/logs/db.rs` → `crates/nucleo/src/db.rs`
- Move: todo o resto de `src/` → `crates/cli/src/`
- Modify: imports em ~8 arquivos do cli (tabela abaixo)

**Interfaces:**
- Produces: crate `nucleo` com `nucleo::core::*` e `nucleo::db::*` públicos; bin `dev-cli` no pacote `dev-cli` (`cargo run -p dev-cli`).

- [ ] **Step 1.1: Criar diretórios e mover arquivos com git mv**

```bash
mkdir -p crates/nucleo/src crates/cli/src
git mv src/logs/core.rs crates/nucleo/src/core.rs
git mv src/logs/db.rs crates/nucleo/src/db.rs
git mv src/main.rs src/cli.rs src/help.rs src/tui.rs crates/cli/src/
git mv src/ai crates/cli/src/ai
git mv src/logs crates/cli/src/logs
git mv src/screens crates/cli/src/screens
rm -f src/.DS_Store && rmdir src
```

- [ ] **Step 1.2: Reescrever o Cargo.toml da raiz (workspace)**

Substitua TODO o conteúdo de `Cargo.toml` (raiz) por:

```toml
# Raiz do workspace: não há mais [package] aqui — cada crate em crates/*
# tem o seu. O resolver "3" é o padrão da edition 2024.
# docs: https://doc.rust-lang.org/cargo/reference/workspaces.html
[workspace]
resolver = "3"
members = ["crates/nucleo", "crates/cli"]

# Campos herdados pelos crates com `.workspace = true` (evita repetir
# versão/edition em cada Cargo.toml).
# docs: https://doc.rust-lang.org/cargo/reference/workspaces.html#the-package-table
[workspace.package]
version = "0.2.0"
edition = "2024"
license = "MIT"
repository = "https://github.com/jarede/dev-cli"

# Dependências compartilhadas: os crates referenciam com
# `dep.workspace = true`, garantindo UMA versão única no workspace inteiro.
# docs: https://doc.rust-lang.org/cargo/reference/workspaces.html#the-dependencies-table
[workspace.dependencies]
chrono = "0.4"
clap = { version = "4.6.1", features = ["derive"] }
crossterm = "0.28"
owo-colors = "4"
ratatui = "0.29"
reqwest = { version = "0.13", features = ["blocking", "json"] }
rusqlite = { version = "0.40", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
walkdir = "2.5"
nucleo = { path = "crates/nucleo" }
```

- [ ] **Step 1.3: Criar crates/nucleo/Cargo.toml**

```toml
[package]
name = "nucleo"
description = "Núcleo do dev-cli: parse de logs, métricas, coleta docker e SQLite"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
rusqlite.workspace = true
```

- [ ] **Step 1.4: Criar crates/cli/Cargo.toml**

```toml
[package]
name = "dev-cli"
description = "Canivete suíço de linha de comando para tarefas de desenvolvimento"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

# O binário se chama dev-cli e o entry point segue em src/main.rs.
[[bin]]
name = "dev-cli"
path = "src/main.rs"

[dependencies]
chrono.workspace = true
clap.workspace = true
crossterm.workspace = true
nucleo.workspace = true
owo-colors.workspace = true
ratatui.workspace = true
reqwest.workspace = true
rusqlite.workspace = true
serde.workspace = true
serde_json.workspace = true
walkdir.workspace = true
```

- [ ] **Step 1.5: Criar crates/nucleo/src/lib.rs**

```rust
// Raiz da lib `nucleo`: o coração do dev-cli, sem NENHUMA dependência de
// terminal (clap/ratatui/cores). A regra de arquitetura do projeto:
//   - NÚCLEO PURO (core, metricas): funções texto/valores -> valores,
//     100% testáveis com strings inline;
//   - CASCA DE IO (db, executor, coletor): SQLite, processos docker/ssh;
//   - APRESENTAÇÃO fica nos binários (crates/cli renderiza; o futuro
//     crates/servidor serializa JSON).
// Num workspace, `pub mod` aqui é o que torna cada módulo visível para os
// OUTROS crates (diferente de `pub(crate)`, que restringe ao próprio crate).
// docs: https://doc.rust-lang.org/reference/visibility-and-privacy.html
pub mod core;
pub mod db;
```

- [ ] **Step 1.6: Tornar pública a API do nucleo**

Nos DOIS arquivos `crates/nucleo/src/core.rs` e `crates/nucleo/src/db.rs`, troque toda ocorrência de `pub(crate)` por `pub` (agora são API pública da lib):

```bash
sed -i '' 's/pub(crate)/pub/g' crates/nucleo/src/core.rs crates/nucleo/src/db.rs
```

- [ ] **Step 1.7: Mover `exibir_estatisticas` do nucleo para o cli**

A função `exibir_estatisticas` em `crates/nucleo/src/db.rs` usa `renderizar_container` (cores/owo-colors = apresentação), que fica no cli. Recorte a função INTEIRA `pub fn exibir_estatisticas(...)` (a última do arquivo, com seus comentários) de `crates/nucleo/src/db.rs` e cole no FINAL de `crates/cli/src/logs/render.rs`, mudando a assinatura para `pub(crate)`. Remova também de `db.rs` a linha `use crate::logs::render::renderizar_container;`.

No topo de `crates/cli/src/logs/render.rs`, adicione os imports que a função usa:

```rust
use rusqlite::Connection;
```

E dentro da função colada, o `renderizar_container` já está no mesmo módulo (sem `use` necessário). A função usa `BTreeMap`, que `render.rs` já importa.

- [ ] **Step 1.8: Corrigir imports no crate cli**

Tabela de trocas (edite cada arquivo):

| Arquivo | Linha atual | Nova linha |
|---|---|---|
| `crates/cli/src/screens/app_types.rs` | `use crate::logs::core::{analisar_apps, parse_loguru_line, AppType};` | `use nucleo::core::{analisar_apps, parse_loguru_line, AppType};` |
| `crates/cli/src/screens/lines.rs` | `use crate::logs::core::{format_loguru_entry, parse_loguru_line};` | `use nucleo::core::{format_loguru_entry, parse_loguru_line};` |
| `crates/cli/src/screens/loguru_stats.rs` | `use crate::logs::core::{parse_loguru_line, LoguruEntry};` | `use nucleo::core::{parse_loguru_line, LoguruEntry};` |
| `crates/cli/src/logs/stats.rs` | `use crate::logs::core::contar;` | `use nucleo::core::contar;` |
| `crates/cli/src/logs/containers.rs` | `use crate::logs::core::contar_niveis_container;` | `use nucleo::core::contar_niveis_container;` |
| `crates/cli/src/logs/remote.rs` | `use crate::logs::core::{categorizar_por_nivel, contar_niveis_docker};` | `use nucleo::core::{categorizar_por_nivel, contar_niveis_docker};` |
| `crates/cli/src/logs/remote.rs` | `use crate::logs::db::{ armazenar_contagens, armazenar_linhas, exibir_estatisticas, init_db, verificar_status_containers, };` | `use nucleo::db::{armazenar_contagens, armazenar_linhas, init_db, verificar_status_containers};` e adicione `use crate::logs::render::exibir_estatisticas;` |
| `crates/cli/src/logs/mod.rs` | `db::init_db(&conn)?;` | `nucleo::db::init_db(&conn)?;` |
| `crates/cli/src/logs/mod.rs` | `mod db;` (na lista de `mod` do topo) | REMOVER a linha (db agora vem do nucleo) |
| `crates/cli/src/logs/mod.rs` | `pub(crate) mod core;` | REMOVER a linha (core agora vem do nucleo) |
| `crates/cli/src/logs/render.rs` | `use crate::logs::core::Contagens;` | `use nucleo::core::Contagens;` |

- [ ] **Step 1.9: Compilar e iterar até zero erros**

Run: `cargo build 2>&1 | tail -20`

Se aparecerem erros de import/visibilidade que a tabela não cobriu, aplique a mesma regra: `crate::logs::core` → `nucleo::core`, `crate::logs::db` → `nucleo::db`; itens do nucleo devem ser `pub`. NÃO mude lógica.

Expected ao final: `Finished \`dev\` profile`

- [ ] **Step 1.10: Testes + clippy + smoke test**

Run: `cargo test --workspace 2>&1 | tail -5`
Expected: `test result: ok.` (mesma contagem de testes de antes)

Run: `cargo clippy --workspace -- -D warnings 2>&1 | tail -3`
Expected: sem warnings.

Run: `cargo run -q -p dev-cli -- version`
Expected: `dev-cli 0.2.0`

- [ ] **Step 1.11: Commit**

```bash
git add -A && git commit -m "refactor: reestrutura em workspace com crates nucleo e cli"
```

---

### Task 2: Configuração (TOML + env DEV_CLI_*)

**Files:**
- Create: `crates/nucleo/src/config.rs`
- Modify: `crates/nucleo/src/lib.rs` (adicionar `pub mod config;`)
- Modify: `crates/nucleo/Cargo.toml` (deps serde, toml, dotenvy, dirs)
- Test: testes inline em `config.rs`

**Interfaces:**
- Produces: `nucleo::config::Config { coleta: Coleta, limiares: Limiares }`, `Config::de_toml(&str) -> Result<Config, toml::de::Error>`, `Config::aplicar_env(&mut self, vars)`, `Config::carregar(Option<&Path>) -> Result<Config, Box<dyn Error>>`, `Config::caminho_db(&self) -> PathBuf`.
- `Coleta { intervalo_seg: u64, janela_min: u64, retencao_horas: u64, tail_inicial: usize, db: String, ssh: String }`
- `Limiares { p95_lento_seg: f64, taxa_erro_pct: f64 }`

- [ ] **Step 2.1: Adicionar dependências ao nucleo**

Em `Cargo.toml` (raiz), adicione ao `[workspace.dependencies]`:

```toml
dirs = "6"
dotenvy = "0.15"
toml = "0.9"
```

Em `crates/nucleo/Cargo.toml`, seção `[dependencies]`, adicione:

```toml
dirs.workspace = true
dotenvy.workspace = true
serde.workspace = true
toml.workspace = true
```

- [ ] **Step 2.2: Escrever config.rs com os testes**

Crie `crates/nucleo/src/config.rs` com este conteúdo completo:

```rust
// Configuração do dev-cli. O projeto é público/open source: NADA de host,
// caminho ou credencial hardcoded no código — tudo vem daqui.
//
// Precedência (do mais forte para o mais fraco):
//   flags da CLI  >  variáveis de ambiente DEV_CLI_*  >  arquivo TOML  >  defaults
//
// A parte PURA (parse do TOML, aplicação de env vars sobre a struct) é
// testável com strings inline; a CASCA (`carregar`) lê arquivo/ambiente.

use std::path::{Path, PathBuf};

// `Deserialize`: derive do serde que gera o código de conversão
// TOML -> struct em tempo de compilação.
// docs: https://docs.rs/serde/latest/serde/trait.Deserialize.html
use serde::Deserialize;

/// Configuração completa, espelhando o arquivo TOML:
///
/// ```toml
/// [coleta]
/// intervalo_seg = 60
/// janela_min = 15
/// retencao_horas = 24
/// tail_inicial = 1000
/// db = "~/.local/share/dev-cli/logs.db"
/// ssh = ""            # vazio = docker local; "user@host" = via SSH
///
/// [limiares]
/// p95_lento_seg = 1.0
/// taxa_erro_pct = 5.0
/// ```
// `#[serde(default)]`: se uma seção/campo faltar no TOML, usa o `Default`
// correspondente em vez de falhar — permite arquivos parciais.
// docs: https://serde.rs/container-attrs.html#default
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub coleta: Coleta,
    pub limiares: Limiares,
}

/// Parâmetros da coleta de logs.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Coleta {
    /// Segundos entre um ciclo de coleta e o próximo.
    pub intervalo_seg: u64,
    /// Janela (minutos) considerada nas estatísticas do dashboard.
    pub janela_min: u64,
    /// Depois de quantas horas linhas/requests antigas são apagadas do banco.
    pub retencao_horas: u64,
    /// Linhas do `docker logs --tail` na PRIMEIRA coleta de um container
    /// (as seguintes são incrementais com `--since`).
    pub tail_inicial: usize,
    /// Caminho do SQLite. Vazio = default do sistema (~/.local/share/dev-cli/logs.db).
    pub db: String,
    /// Host SSH ("user@host"). Vazio = docker local (modo padrão na VM).
    pub ssh: String,
}

// `Default` implementado à mão (em vez de `#[derive(Default)]`) porque os
// defaults do projeto NÃO são os "zero values" do Rust (0, "").
// docs: https://doc.rust-lang.org/std/default/trait.Default.html
impl Default for Coleta {
    fn default() -> Self {
        Self {
            intervalo_seg: 60,
            janela_min: 15,
            retencao_horas: 24,
            tail_inicial: 1000,
            db: String::new(),
            ssh: String::new(),
        }
    }
}

/// Limiares que separam verde/amarelo/vermelho no dashboard.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Limiares {
    /// p95 de tempo de resposta (segundos) acima do qual o container fica amarelo.
    pub p95_lento_seg: f64,
    /// Percentual de linhas ERROR/CRIT acima do qual o container fica vermelho.
    pub taxa_erro_pct: f64,
}

impl Default for Limiares {
    fn default() -> Self {
        Self {
            p95_lento_seg: 1.0,
            taxa_erro_pct: 5.0,
        }
    }
}

impl Config {
    /// NÚCLEO PURO: parseia um TOML (possivelmente parcial) numa Config.
    pub fn de_toml(texto: &str) -> Result<Config, toml::de::Error> {
        // `toml::from_str` usa o `Deserialize` derivado acima.
        // docs: https://docs.rs/toml/latest/toml/fn.from_str.html
        toml::from_str(texto)
    }

    /// NÚCLEO PURO: aplica variáveis de ambiente `DEV_CLI_*` sobre a config.
    /// Recebe as vars como iterador de pares (nome, valor) — em produção vem
    /// de `std::env::vars()`, nos testes de um `Vec` inline.
    /// Valores que não parseiam (ex.: "abc" para um número) são ignorados
    /// silenciosamente: preferimos rodar com o default a abortar por causa
    /// de uma env var malformada de outro processo.
    pub fn aplicar_env<I>(&mut self, vars: I)
    where
        // `IntoIterator`: aceita Vec, array, iterador de env::vars()...
        // docs: https://doc.rust-lang.org/std/iter/trait.IntoIterator.html
        I: IntoIterator<Item = (String, String)>,
    {
        for (nome, valor) in vars {
            // `match` + `if let Ok(...)` = só sobrescreve quando o parse dá certo.
            match nome.as_str() {
                "DEV_CLI_COLETA_INTERVALO_SEG" => {
                    if let Ok(n) = valor.parse() {
                        self.coleta.intervalo_seg = n;
                    }
                }
                "DEV_CLI_COLETA_JANELA_MIN" => {
                    if let Ok(n) = valor.parse() {
                        self.coleta.janela_min = n;
                    }
                }
                "DEV_CLI_COLETA_RETENCAO_HORAS" => {
                    if let Ok(n) = valor.parse() {
                        self.coleta.retencao_horas = n;
                    }
                }
                "DEV_CLI_COLETA_TAIL_INICIAL" => {
                    if let Ok(n) = valor.parse() {
                        self.coleta.tail_inicial = n;
                    }
                }
                "DEV_CLI_COLETA_DB" => self.coleta.db = valor,
                "DEV_CLI_COLETA_SSH" => self.coleta.ssh = valor,
                "DEV_CLI_LIMIARES_P95_LENTO_SEG" => {
                    if let Ok(n) = valor.parse() {
                        self.limiares.p95_lento_seg = n;
                    }
                }
                "DEV_CLI_LIMIARES_TAXA_ERRO_PCT" => {
                    if let Ok(n) = valor.parse() {
                        self.limiares.taxa_erro_pct = n;
                    }
                }
                _ => {}
            }
        }
    }

    /// CASCA DE IO: resolve a config completa respeitando a precedência
    /// arquivo < env. (As flags da CLI são aplicadas pelo chamador, que é
    /// quem as conhece.) Arquivo ausente NÃO é erro (usa defaults); arquivo
    /// presente mas inválido É erro (avisa o usuário em vez de ignorar).
    pub fn carregar(caminho: Option<&Path>) -> Result<Config, Box<dyn std::error::Error>> {
        // Carrega um `.env` do diretório atual, se existir (útil em dev).
        // O resultado é ignorado de propósito: sem `.env` não é erro.
        // docs: https://docs.rs/dotenvy/latest/dotenvy/fn.dotenv.html
        let _ = dotenvy::dotenv();

        // `--config` explícito > caminho padrão do sistema.
        // `dirs::config_dir()`: ~/.config no Linux, ~/Library/Application Support no macOS.
        // docs: https://docs.rs/dirs/latest/dirs/fn.config_dir.html
        let caminho_arquivo: Option<PathBuf> = match caminho {
            Some(c) => Some(c.to_path_buf()),
            None => dirs::config_dir().map(|d| d.join("dev-cli").join("config.toml")),
        };

        let mut config = match caminho_arquivo {
            Some(c) if c.exists() => {
                let texto = std::fs::read_to_string(&c)?;
                Config::de_toml(&texto)
                    .map_err(|erro| format!("config inválida em {}: {erro}", c.display()))?
            }
            // Sem arquivo: começa dos defaults.
            _ => Config::default(),
        };

        config.aplicar_env(std::env::vars());
        Ok(config)
    }

    /// Resolve o caminho final do banco: campo `db` (com `~` expandido) ou
    /// o diretório de dados do sistema (~/.local/share/dev-cli/logs.db).
    pub fn caminho_db(&self) -> PathBuf {
        if self.coleta.db.is_empty() {
            return dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("dev-cli")
                .join("logs.db");
        }
        expandir_til(&self.coleta.db)
    }
}

/// Expande um `~/` inicial para o diretório home do usuário.
/// (O shell faz isso em argumentos de linha de comando, mas NÃO em valores
/// vindos de arquivo de config — por isso fazemos manualmente.)
fn expandir_til(caminho: &str) -> PathBuf {
    if let Some(resto) = caminho.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(resto);
    }
    PathBuf::from(caminho)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_sao_os_documentados() {
        let c = Config::default();
        assert_eq!(c.coleta.intervalo_seg, 60);
        assert_eq!(c.coleta.janela_min, 15);
        assert_eq!(c.coleta.retencao_horas, 24);
        assert_eq!(c.coleta.tail_inicial, 1000);
        assert_eq!(c.coleta.db, "");
        assert_eq!(c.coleta.ssh, "");
        assert_eq!(c.limiares.p95_lento_seg, 1.0);
        assert_eq!(c.limiares.taxa_erro_pct, 5.0);
    }

    #[test]
    fn toml_parcial_preserva_defaults_do_resto() {
        let c = Config::de_toml("[coleta]\nintervalo_seg = 30\n").unwrap();
        assert_eq!(c.coleta.intervalo_seg, 30);
        // O resto continua nos defaults.
        assert_eq!(c.coleta.janela_min, 15);
        assert_eq!(c.limiares.taxa_erro_pct, 5.0);
    }

    #[test]
    fn toml_completo() {
        let texto = r#"
[coleta]
intervalo_seg = 10
janela_min = 5
retencao_horas = 48
tail_inicial = 200
db = "/tmp/x.db"
ssh = "eu@host"

[limiares]
p95_lento_seg = 2.5
taxa_erro_pct = 1.0
"#;
        let c = Config::de_toml(texto).unwrap();
        assert_eq!(c.coleta.ssh, "eu@host");
        assert_eq!(c.coleta.db, "/tmp/x.db");
        assert_eq!(c.limiares.p95_lento_seg, 2.5);
    }

    #[test]
    fn toml_invalido_e_erro() {
        assert!(Config::de_toml("isto nao é toml [").is_err());
    }

    #[test]
    fn env_sobrepoe_arquivo() {
        let mut c = Config::de_toml("[coleta]\nintervalo_seg = 30\n").unwrap();
        c.aplicar_env(vec![
            ("DEV_CLI_COLETA_INTERVALO_SEG".to_string(), "10".to_string()),
            ("DEV_CLI_COLETA_SSH".to_string(), "dev@qa".to_string()),
            ("DEV_CLI_LIMIARES_TAXA_ERRO_PCT".to_string(), "2.5".to_string()),
        ]);
        assert_eq!(c.coleta.intervalo_seg, 10);
        assert_eq!(c.coleta.ssh, "dev@qa");
        assert_eq!(c.limiares.taxa_erro_pct, 2.5);
    }

    #[test]
    fn env_com_valor_invalido_e_ignorada() {
        let mut c = Config::default();
        c.aplicar_env(vec![(
            "DEV_CLI_COLETA_INTERVALO_SEG".to_string(),
            "abc".to_string(),
        )]);
        assert_eq!(c.coleta.intervalo_seg, 60); // manteve o default
    }

    #[test]
    fn env_desconhecida_e_ignorada() {
        let mut c = Config::default();
        c.aplicar_env(vec![("PATH".to_string(), "/usr/bin".to_string())]);
        assert_eq!(c, Config::default());
    }

    #[test]
    fn caminho_db_expande_til() {
        let mut c = Config::default();
        c.coleta.db = "~/x/logs.db".to_string();
        let caminho = c.caminho_db();
        assert!(caminho.ends_with("x/logs.db"));
        assert!(!caminho.to_string_lossy().contains('~'));
    }
}
```

- [ ] **Step 2.3: Registrar o módulo**

Em `crates/nucleo/src/lib.rs`, adicione após `pub mod core;`:

```rust
pub mod config;
```

- [ ] **Step 2.4: Rodar os testes**

Run: `cargo test -p nucleo config 2>&1 | tail -5`
Expected: 8 testes de config, `test result: ok.`

Run: `cargo clippy --workspace -- -D warnings 2>&1 | tail -3`
Expected: sem warnings.

- [ ] **Step 2.5: Commit**

```bash
git add -A && git commit -m "feat(nucleo): adiciona configuração via TOML e variáveis DEV_CLI_*"
```

---

### Task 3: Executor docker (local | ssh) e refactor do `logs remote`

**Files:**
- Create: `crates/nucleo/src/executor.rs`
- Modify: `crates/nucleo/src/lib.rs` (adicionar `pub mod executor;`)
- Modify: `crates/cli/src/logs/remote.rs` (usar o executor; remover host hardcoded)
- Test: testes inline em `executor.rs`

**Interfaces:**
- Produces: `nucleo::executor::Executor` (enum `Local | Ssh(String)`), `Executor::montar_comando(&self, &[&str]) -> (String, Vec<String>)` (puro), `Executor::executar(&self, &[&str]) -> Result<String, Box<dyn Error>>` (IO), `ContainerDocker { nome, status, criado_em }` (todos `pub String`), `listar_containers(&Executor) -> Result<Vec<ContainerDocker>, _>`, `obter_logs(&Executor, nome: &str, tail: usize) -> Result<String, _>`, `obter_logs_desde(&Executor, nome: &str, desde: i64) -> Result<String, _>`.
- Consumes: nada de tasks anteriores (independente da config).

- [ ] **Step 3.1: Escrever executor.rs (testes inclusos)**

Crie `crates/nucleo/src/executor.rs`:

```rust
// CASCA DE IO (com miolo puro testável): executa comandos `docker` no host
// local — o modo padrão, já que os binários rodam NA VM que tem o docker —
// ou através de SSH (modo de desenvolvimento, para consultar uma VM remota
// sem instalar nada nela).
//
// A separação chave: `montar_comando` é PURO (decide programa + argumentos,
// testável sem docker/ssh instalados); `executar` é a casca que de fato
// dispara o processo e captura a saída.

use std::process::Command;

/// Estratégia de execução dos comandos docker.
// Um enum com dados ("Ssh" carrega o host) é a forma idiomática em Rust de
// modelar "uma escolha entre alternativas que carregam informação própria" —
// o `match` obriga a tratar todas.
// docs: https://doc.rust-lang.org/book/ch06-01-defining-an-enum.html
#[derive(Debug, Clone, PartialEq)]
pub enum Executor {
    /// Executa `docker ...` diretamente (requer usuário no grupo docker).
    Local,
    /// Executa `ssh <host> "docker ..."` (host no formato "user@host").
    Ssh(String),
}

impl Executor {
    /// NÚCLEO PURO: monta (programa, argumentos) sem executar nada.
    /// No modo SSH os argumentos docker viram UMA string (o shell remoto
    /// re-divide), por isso o `join(" ")`.
    pub fn montar_comando(&self, args_docker: &[&str]) -> (String, Vec<String>) {
        match self {
            Executor::Local => (
                "docker".to_string(),
                args_docker.iter().map(|s| s.to_string()).collect(),
            ),
            Executor::Ssh(host) => (
                "ssh".to_string(),
                vec![host.clone(), format!("docker {}", args_docker.join(" "))],
            ),
        }
    }

    /// CASCA DE IO: executa e devolve stdout+stderr combinados.
    /// Por que combinar? `docker logs` manda para o stderr o que o processo
    /// do container escreveu em stderr — e loggers como o Loguru escrevem
    /// justamente lá. Se o comando FALHOU (exit != 0), o stderr vira mensagem
    /// de erro em vez de dado.
    // docs: https://doc.rust-lang.org/std/process/struct.Command.html
    pub fn executar(&self, args_docker: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
        let (programa, args) = self.montar_comando(args_docker);
        let saida = Command::new(&programa)
            .args(&args)
            .output()
            .map_err(|erro| format!("falha ao executar {programa}: {erro}"))?;

        if !saida.status.success() {
            return Err(format!(
                "`{programa} {}` terminou com erro: {}",
                args.join(" "),
                String::from_utf8_lossy(&saida.stderr)
            )
            .into());
        }

        // `from_utf8_lossy` troca bytes inválidos por U+FFFD em vez de
        // falhar — logs de container nem sempre são UTF-8 perfeito.
        // docs: https://doc.rust-lang.org/std/string/struct.String.html#method.from_utf8_lossy
        let mut texto = String::from_utf8_lossy(&saida.stdout).to_string();
        texto.push_str(&String::from_utf8_lossy(&saida.stderr));
        Ok(texto)
    }
}

/// Metadados de um container obtidos via `docker ps`.
#[derive(Debug, Clone)]
pub struct ContainerDocker {
    pub nome: String,
    /// Status textual: "Up 2 days", "Exited (0) 3 days ago", etc.
    pub status: String,
    /// Timestamp de criação: "2026-07-04 12:00:00 +0000 UTC".
    pub criado_em: String,
}

/// Lista os containers rodando (nome|status|criado_em, um por linha).
pub fn listar_containers(
    executor: &Executor,
) -> Result<Vec<ContainerDocker>, Box<dyn std::error::Error>> {
    let saida = executor.executar(&[
        "ps",
        "--format",
        "'{{.Names}}|{{.Status}}|{{.CreatedAt}}'",
    ])?;
    Ok(parsear_ps(&saida))
}

/// NÚCLEO PURO: converte a saída do `docker ps --format` em structs.
/// Aceita as linhas com ou sem as aspas simples que o format acima gera.
fn parsear_ps(saida: &str) -> Vec<ContainerDocker> {
    saida
        .lines()
        .map(|linha| linha.trim().trim_matches('\''))
        .filter(|linha| !linha.is_empty())
        // `filter_map` + `?` dentro do closure: linhas sem os 3 campos
        // separados por `|` são simplesmente descartadas.
        // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.filter_map
        .filter_map(|linha| {
            let mut partes = linha.splitn(3, '|');
            Some(ContainerDocker {
                nome: partes.next()?.to_string(),
                status: partes.next()?.to_string(),
                criado_em: partes.next()?.to_string(),
            })
        })
        .collect()
}

/// Busca as últimas `tail` linhas do log (0 = todas).
pub fn obter_logs(
    executor: &Executor,
    nome: &str,
    tail: usize,
) -> Result<String, Box<dyn std::error::Error>> {
    let tail_str = tail.to_string();
    if tail > 0 {
        executor.executar(&["logs", "--tail", &tail_str, nome])
    } else {
        executor.executar(&["logs", nome])
    }
}

/// Busca os logs desde um timestamp Unix (coleta incremental).
pub fn obter_logs_desde(
    executor: &Executor,
    nome: &str,
    desde: i64,
) -> Result<String, Box<dyn std::error::Error>> {
    let desde_str = desde.to_string();
    executor.executar(&["logs", "--since", &desde_str, nome])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monta_comando_local() {
        let (prog, args) = Executor::Local.montar_comando(&["logs", "--tail", "10", "meu-app"]);
        assert_eq!(prog, "docker");
        assert_eq!(args, vec!["logs", "--tail", "10", "meu-app"]);
    }

    #[test]
    fn monta_comando_ssh_junta_args_docker() {
        let exec = Executor::Ssh("dev@qa.exemplo.com".to_string());
        let (prog, args) = exec.montar_comando(&["ps", "-a"]);
        assert_eq!(prog, "ssh");
        assert_eq!(args, vec!["dev@qa.exemplo.com", "docker ps -a"]);
    }

    #[test]
    fn parseia_saida_do_ps() {
        let saida = "'web-1|Up 2 days|2026-07-04 12:00:00 +0000 UTC'\n'api-1|Up 5 hours|2026-07-06 08:00:00 +0000 UTC'\n";
        let lista = parsear_ps(saida);
        assert_eq!(lista.len(), 2);
        assert_eq!(lista[0].nome, "web-1");
        assert_eq!(lista[0].status, "Up 2 days");
        assert_eq!(lista[1].criado_em, "2026-07-06 08:00:00 +0000 UTC");
    }

    #[test]
    fn parseia_ps_ignora_linhas_vazias_e_malformadas() {
        let lista = parsear_ps("\n\nsem-pipe\n'a|b|c'\n");
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].nome, "a");
    }
}
```

- [ ] **Step 3.2: Registrar o módulo e rodar os testes**

Em `crates/nucleo/src/lib.rs`, adicione:

```rust
pub mod executor;
```

Run: `cargo test -p nucleo executor 2>&1 | tail -5`
Expected: 4 testes, `test result: ok.`

- [ ] **Step 3.3: Refatorar remote.rs para usar o executor**

Em `crates/cli/src/logs/remote.rs`:

1. Adicione o import: `use nucleo::executor::{listar_containers, obter_logs, obter_logs_desde, ContainerDocker, Executor};`
2. DELETE as três funções locais `obter_logs_remoto_desde`, `listar_containers_remoto`, `obter_logs_remoto` e a struct `ContainerRemoto` (o executor as substitui).
3. Troque o campo `host` da struct `RemoteArgs` — de:

```rust
    /// Host SSH (user@host).
    #[arg(long, default_value = "jarede.silva@qa.bistek.com.br")]
    host: String,
```

para:

```rust
    /// Host SSH ("user@host") para coletar de uma VM remota.
    /// Sem esta flag, executa `docker` localmente (modo padrão na VM).
    #[arg(long)]
    ssh: Option<String>,
```

4. No começo de `execute()`, logo após a abertura, crie o executor:

```rust
        // Decide a estratégia: SSH só quando pedido; o padrão é docker local
        // (os binários rodam na própria VM que tem o docker).
        let executor = match &self.ssh {
            Some(host) => Executor::Ssh(host.clone()),
            None => Executor::Local,
        };
```

5. Substitua TODAS as chamadas no corpo de `execute()`:
   - `listar_containers_remoto(&self.host)?` → `listar_containers(&executor)?`
   - `obter_logs_remoto(&self.host, &c.nome, self.tail)?` → `obter_logs(&executor, &c.nome, self.tail)?`
   - `obter_logs_remoto_desde(&self.host, &c.nome, ultima_coleta)?` → `obter_logs_desde(&executor, &c.nome, ultima_coleta)?`
   - Toda menção ao tipo `ContainerRemoto` → `ContainerDocker`

- [ ] **Step 3.4: Compilar, testar, clippy**

Run: `cargo build --workspace && cargo test --workspace 2>&1 | tail -3 && cargo clippy --workspace -- -D warnings 2>&1 | tail -3`
Expected: build ok, testes ok, clippy limpo.

Smoke test (um container qualquer local, se houver docker; senão pule):
Run: `cargo run -q -p dev-cli -- logs remote --help`
Expected: help mostra `--ssh` (e NÃO mostra default de host bistek).

- [ ] **Step 3.5: Commit**

```bash
git add -A && git commit -m "feat(nucleo): executor docker local/ssh substitui host hardcoded"
```

---

### Task 4: Tabela `requests` + métricas puras (p95, severidade, resumo)

**Files:**
- Modify: `crates/nucleo/src/db.rs` (tabela requests, armazenar_requests, prune, resumo_janela; armazenar_linhas sem DELETE)
- Create: `crates/nucleo/src/metricas.rs`
- Modify: `crates/nucleo/src/lib.rs` (adicionar `pub mod metricas;`)
- Test: testes inline em `metricas.rs` e `db.rs`

**Interfaces:**
- Consumes: `core::LoguruEntry` (campos `metodo, status, path, duracao_seg, tenant, timestamp` — já existem), `config::Limiares` (Task 2).
- Produces:
  - `metricas::p95(&[f64]) -> Option<f64>`
  - `metricas::ResumoContainer { nome: String, status: String, uptime: String, erros: i64, crits: i64, c5xx: i64, c4xx: i64, reqs: i64, p95_seg: Option<f64>, max_seg: Option<f64>, total_linhas: i64, ultima_coleta: i64 }` (todos os campos `pub`)
  - `metricas::Severidade { Verde, Amarelo, Vermelho, Parado }` (deriva `Ord`; Parado é o maior)
  - `metricas::severidade(&ResumoContainer, &Limiares) -> Severidade`
  - `db::armazenar_requests(&Connection, nome: &str, entradas: &[LoguruEntry], agora: i64) -> Result<(), _>`
  - `db::prune_antigos(&Connection, corte: i64) -> Result<(), _>`
  - `db::resumo_janela(&Connection, corte: i64) -> Result<Vec<ResumoContainer>, _>`

- [ ] **Step 4.1: Escrever metricas.rs (testes inclusos)**

Crie `crates/nucleo/src/metricas.rs`:

```rust
// NÚCLEO PURO: estatísticas e classificação de severidade dos containers.
// Nenhuma função aqui faz IO — tudo recebe valores e devolve valores, o que
// permite testar 100% dos caminhos com dados inline.

use crate::config::Limiares;

/// Percentil 95 das durações (segundos). `None` para lista vazia.
/// Método "nearest-rank": ordena e pega o elemento na posição
/// ceil(0.95 * n) — simples e suficiente para um dashboard.
// `&[f64]` (slice) em vez de `&Vec<f64>`: aceita Vec, array, fatia... e
// deixa claro que só LEMOS os dados (o clone+sort acontece dentro).
// docs: https://doc.rust-lang.org/book/ch04-03-slices.html
pub fn p95(duracoes: &[f64]) -> Option<f64> {
    if duracoes.is_empty() {
        return None;
    }
    let mut ordenadas = duracoes.to_vec();
    // f64 não implementa `Ord` (NaN quebra a ordem total), então usamos
    // `sort_by` com `total_cmp`, que define uma ordem total para floats.
    // docs: https://doc.rust-lang.org/std/primitive.f64.html#method.total_cmp
    ordenadas.sort_by(|a, b| a.total_cmp(b));
    let n = ordenadas.len();
    let posicao = ((n as f64) * 0.95).ceil() as usize;
    // `saturating_sub(1)`: converte posição 1-based em índice 0-based sem
    // risco de underflow quando n == 1.
    // docs: https://doc.rust-lang.org/std/primitive.usize.html#method.saturating_sub
    Some(ordenadas[posicao.saturating_sub(1).min(n - 1)])
}

/// Tudo que o dashboard mostra sobre um container, já agregado na janela.
#[derive(Debug, Clone, Default)]
pub struct ResumoContainer {
    pub nome: String,
    /// "running" ou "stopped" (coluna `status` da tabela containers).
    pub status: String,
    /// Texto de uptime do docker ps ("Up 2 days"); vazio se desconhecido.
    pub uptime: String,
    /// Linhas de nível ERROR/ERRO na janela.
    pub erros: i64,
    /// Linhas de nível CRITICAL/CRIT/FATAL na janela.
    pub crits: i64,
    /// Requests HTTP com status 5xx na janela.
    pub c5xx: i64,
    /// Requests HTTP com status 4xx na janela.
    pub c4xx: i64,
    /// Total de requests HTTP na janela.
    pub reqs: i64,
    /// p95 do tempo de resposta (segundos) na janela; None sem requests.
    pub p95_seg: Option<f64>,
    /// Maior tempo de resposta (segundos) na janela.
    pub max_seg: Option<f64>,
    /// Total de linhas de log (todos os níveis) na janela.
    pub total_linhas: i64,
    /// Timestamp Unix da última coleta deste container.
    pub ultima_coleta: i64,
}

/// Severidade de um container, da melhor para a pior.
// A ORDEM das variantes importa: `derive(Ord)` ordena pela posição de
// declaração, então Verde < Amarelo < Vermelho < Parado — o dashboard
// ordena decrescente para pôr os piores no topo.
// docs: https://doc.rust-lang.org/std/cmp/trait.Ord.html#derivable
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severidade {
    Verde,
    Amarelo,
    Vermelho,
    Parado,
}

/// Classifica um container:
///   Parado    — não está mais rodando (gravidade máxima);
///   Vermelho  — CRIT presente, OU taxa de ERROR/CRIT ≥ limiar, OU algum 5xx;
///   Amarelo   — p95 ≥ limiar de lentidão, OU há erros abaixo do limiar,
///               OU mais de 10% das requests são 4xx;
///   Verde     — nada acima.
pub fn severidade(resumo: &ResumoContainer, limiares: &Limiares) -> Severidade {
    if resumo.status == "stopped" {
        return Severidade::Parado;
    }

    let taxa_erro = if resumo.total_linhas > 0 {
        ((resumo.erros + resumo.crits) as f64) * 100.0 / (resumo.total_linhas as f64)
    } else {
        0.0
    };
    if resumo.crits > 0 || resumo.c5xx > 0 || taxa_erro >= limiares.taxa_erro_pct {
        return Severidade::Vermelho;
    }

    // Let chain (edition 2024): `Some(p)` E a condição, sem `if` aninhado.
    if let Some(p) = resumo.p95_seg
        && p >= limiares.p95_lento_seg
    {
        return Severidade::Amarelo;
    }
    let muitos_4xx = resumo.reqs > 0 && (resumo.c4xx as f64) > (resumo.reqs as f64) * 0.10;
    if resumo.erros > 0 || muitos_4xx {
        return Severidade::Amarelo;
    }

    Severidade::Verde
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiares() -> Limiares {
        Limiares {
            p95_lento_seg: 1.0,
            taxa_erro_pct: 5.0,
        }
    }

    fn resumo_saudavel() -> ResumoContainer {
        ResumoContainer {
            nome: "app".to_string(),
            status: "running".to_string(),
            total_linhas: 100,
            reqs: 50,
            p95_seg: Some(0.2),
            max_seg: Some(0.5),
            ..Default::default()
        }
    }

    #[test]
    fn p95_de_lista_vazia_e_none() {
        assert_eq!(p95(&[]), None);
    }

    #[test]
    fn p95_de_um_elemento_e_ele_mesmo() {
        assert_eq!(p95(&[0.42]), Some(0.42));
    }

    #[test]
    fn p95_de_cem_elementos_e_o_95o() {
        // 0.01, 0.02, ..., 1.00 -> o 95º valor é 0.95
        let valores: Vec<f64> = (1..=100).map(|i| i as f64 / 100.0).collect();
        assert_eq!(p95(&valores), Some(0.95));
    }

    #[test]
    fn p95_nao_depende_da_ordem_de_entrada() {
        assert_eq!(p95(&[3.0, 1.0, 2.0]), Some(3.0));
    }

    #[test]
    fn container_saudavel_e_verde() {
        assert_eq!(severidade(&resumo_saudavel(), &limiares()), Severidade::Verde);
    }

    #[test]
    fn container_parado_e_parado_mesmo_sem_erros() {
        let mut r = resumo_saudavel();
        r.status = "stopped".to_string();
        assert_eq!(severidade(&r, &limiares()), Severidade::Parado);
    }

    #[test]
    fn crit_e_vermelho() {
        let mut r = resumo_saudavel();
        r.crits = 1;
        assert_eq!(severidade(&r, &limiares()), Severidade::Vermelho);
    }

    #[test]
    fn cincoxx_e_vermelho() {
        let mut r = resumo_saudavel();
        r.c5xx = 1;
        assert_eq!(severidade(&r, &limiares()), Severidade::Vermelho);
    }

    #[test]
    fn taxa_de_erro_acima_do_limiar_e_vermelho() {
        let mut r = resumo_saudavel();
        r.erros = 6; // 6 de 100 linhas = 6% >= 5%
        assert_eq!(severidade(&r, &limiares()), Severidade::Vermelho);
    }

    #[test]
    fn poucos_erros_abaixo_do_limiar_e_amarelo() {
        let mut r = resumo_saudavel();
        r.erros = 2; // 2% < 5%
        assert_eq!(severidade(&r, &limiares()), Severidade::Amarelo);
    }

    #[test]
    fn p95_lento_e_amarelo() {
        let mut r = resumo_saudavel();
        r.p95_seg = Some(1.5);
        assert_eq!(severidade(&r, &limiares()), Severidade::Amarelo);
    }

    #[test]
    fn muitos_4xx_e_amarelo() {
        let mut r = resumo_saudavel();
        r.c4xx = 10; // 10 de 50 = 20% > 10%
        assert_eq!(severidade(&r, &limiares()), Severidade::Amarelo);
    }

    #[test]
    fn severidade_ordena_do_verde_ao_parado() {
        assert!(Severidade::Verde < Severidade::Amarelo);
        assert!(Severidade::Amarelo < Severidade::Vermelho);
        assert!(Severidade::Vermelho < Severidade::Parado);
    }
}
```

- [ ] **Step 4.2: Registrar o módulo e rodar os testes**

Em `crates/nucleo/src/lib.rs`, adicione:

```rust
pub mod metricas;
```

Run: `cargo test -p nucleo metricas 2>&1 | tail -5`
Expected: 13 testes, `test result: ok.`

- [ ] **Step 4.3: Ampliar db.rs — schema, requests, prune, resumo**

Em `crates/nucleo/src/db.rs`:

1. Adicione ao topo:

```rust
use crate::core::LoguruEntry;
use crate::metricas::{p95, ResumoContainer};
```

2. Dentro de `init_db`, no `execute_batch`, adicione ao final da string SQL (antes da aspa de fechamento):

```sql
        CREATE TABLE IF NOT EXISTS requests (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            container_name TEXT NOT NULL,
            ts TEXT NOT NULL,
            metodo TEXT NOT NULL,
            path TEXT NOT NULL,
            status INTEGER NOT NULL,
            duracao_seg REAL NOT NULL,
            tenant TEXT NOT NULL,
            collected_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_requests_container_ts
            ON requests (container_name, collected_at);
        CREATE INDEX IF NOT EXISTS idx_log_lines_container_ts
            ON log_lines (container_name, collected_at);
        CREATE INDEX IF NOT EXISTS idx_log_counts_container_ts
            ON log_counts (container_name, collected_at);
```

3. Em `armazenar_linhas`, DELETE o bloco que apaga as linhas anteriores (o `conn.execute("DELETE FROM log_lines WHERE container_name = ?1", ...)` e seu comentário). A retenção agora é por tempo, via `prune_antigos` — sem isso, o dashboard não conseguiria somar a janela de 15 minutos através de várias coletas.

4. Adicione ao final do arquivo:

```rust
/// Persiste as requests HTTP parseadas (formato Loguru) desta coleta.
pub fn armazenar_requests(
    conn: &Connection,
    nome: &str,
    entradas: &[LoguruEntry],
    agora: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Uma statement preparada reutilizada no loop (mais rápido que preparar
    // SQL novo por linha) — mesmo padrão de `armazenar_contagens`.
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.prepare
    let mut stmt = conn.prepare(
        "INSERT INTO requests (container_name, ts, metodo, path, status, duracao_seg, tenant, collected_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for e in entradas {
        stmt.execute(rusqlite::params![
            nome,
            e.timestamp,
            e.metodo,
            e.path,
            e.status,
            e.duracao_seg,
            e.tenant,
            agora
        ])?;
    }
    Ok(())
}

/// Apaga dados mais antigos que `corte` (timestamp Unix) — a retenção do
/// banco. Chamado a cada ciclo de coleta.
pub fn prune_antigos(conn: &Connection, corte: i64) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute("DELETE FROM log_lines WHERE collected_at < ?1", rusqlite::params![corte])?;
    conn.execute("DELETE FROM requests WHERE collected_at < ?1", rusqlite::params![corte])?;
    conn.execute("DELETE FROM log_counts WHERE collected_at < ?1", rusqlite::params![corte])?;
    conn.execute("DELETE FROM alerts WHERE created_at < ?1", rusqlite::params![corte])?;
    Ok(())
}

/// Monta o resumo por container considerando só a janela `collected_at >= corte`.
/// Contagens vêm do SQL (rápido); p95/máx são calculados em Rust a partir das
/// durações da janela (SQLite não tem percentil nativo).
pub fn resumo_janela(
    conn: &Connection,
    corte: i64,
) -> Result<Vec<ResumoContainer>, Box<dyn std::error::Error>> {
    // 1. Base: todos os containers conhecidos, com status e última coleta.
    let mut stmt = conn.prepare(
        "SELECT name, status, uptime, last_collected_at FROM containers ORDER BY name",
    )?;
    let mut resumos: Vec<ResumoContainer> = stmt
        .query_map([], |r| {
            Ok(ResumoContainer {
                nome: r.get(0)?,
                status: r.get(1)?,
                uptime: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                ultima_coleta: r.get(3)?,
                ..Default::default()
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    for resumo in &mut resumos {
        // 2. Contagens por nível na janela (a partir de log_counts).
        let mut stmt = conn.prepare(
            "SELECT level, SUM(count) FROM log_counts
             WHERE container_name = ?1 AND collected_at >= ?2 GROUP BY level",
        )?;
        let niveis = stmt.query_map(rusqlite::params![resumo.nome, corte], |r| {
            let nivel: String = r.get(0)?;
            let total: i64 = r.get(1)?;
            Ok((nivel, total))
        })?;
        for par in niveis.filter_map(|r| r.ok()) {
            let (nivel, total) = par;
            resumo.total_linhas += total;
            match nivel.to_uppercase().as_str() {
                "ERROR" | "ERRO" => resumo.erros += total,
                "CRITICAL" | "CRIT" | "FATAL" => resumo.crits += total,
                _ => {}
            }
        }

        // 3. Requests na janela: contagens por classe de status via SQL...
        let (reqs, c5xx, c4xx): (i64, i64, i64) = conn.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(status BETWEEN 500 AND 599), 0),
                    COALESCE(SUM(status BETWEEN 400 AND 499), 0)
             FROM requests WHERE container_name = ?1 AND collected_at >= ?2",
            rusqlite::params![resumo.nome, corte],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        resumo.reqs = reqs;
        resumo.c5xx = c5xx;
        resumo.c4xx = c4xx;

        // 4. ...e durações trazidas para o Rust para p95/máx.
        let mut stmt = conn.prepare(
            "SELECT duracao_seg FROM requests
             WHERE container_name = ?1 AND collected_at >= ?2",
        )?;
        let duracoes: Vec<f64> = stmt
            .query_map(rusqlite::params![resumo.nome, corte], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        resumo.p95_seg = p95(&duracoes);
        // `fold` com `f64::max` em vez de `.max()` porque f64 não é `Ord`.
        // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.fold
        resumo.max_seg = if duracoes.is_empty() {
            None
        } else {
            Some(duracoes.iter().fold(f64::MIN, |a, &b| a.max(b)))
        };
    }

    Ok(resumos)
}
```

5. Adicione o módulo de testes no FINAL de `db.rs` (o arquivo ainda não tem testes):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parse_loguru_line;

    /// Banco em memória com o schema criado — cada teste parte do zero.
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.open_in_memory
    fn banco() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    fn inserir_container(conn: &Connection, nome: &str, status: &str, agora: i64) {
        conn.execute(
            "INSERT OR REPLACE INTO containers (name, status, last_collected_at, uptime, criado_em)
             VALUES (?1, ?2, ?3, 'Up 1 day', '')",
            rusqlite::params![nome, status, agora],
        )
        .unwrap();
    }

    #[test]
    fn resumo_janela_agrega_contagens_e_requests() {
        let conn = banco();
        inserir_container(&conn, "app", "running", 1000);

        // Contagens: 2 ERROR + 8 INFO dentro da janela, 5 ERROR fora.
        let mut niveis = std::collections::BTreeMap::new();
        niveis.insert("ERROR".to_string(), 2usize);
        niveis.insert("INFO".to_string(), 8usize);
        armazenar_contagens(&conn, "app", &niveis, 1000).unwrap();
        let mut antigos = std::collections::BTreeMap::new();
        antigos.insert("ERROR".to_string(), 5usize);
        armazenar_contagens(&conn, "app", &antigos, 10).unwrap();

        // Uma request 200 e uma 500 dentro da janela (linha Loguru real).
        let linha = "2026-07-07 10:00:00.000 |INFO     | server:http_request:112 - [acme] GET 200 /api/x  0.150s [10.0.0.1] [curl]";
        let e200 = parse_loguru_line(linha).unwrap();
        let mut e500 = e200.clone();
        e500.status = 500;
        e500.duracao_seg = 2.0;
        armazenar_requests(&conn, "app", &[e200, e500], 1000).unwrap();

        let resumos = resumo_janela(&conn, 500).unwrap();
        assert_eq!(resumos.len(), 1);
        let r = &resumos[0];
        assert_eq!(r.nome, "app");
        assert_eq!(r.erros, 2); // os 5 antigos ficaram fora da janela
        assert_eq!(r.total_linhas, 10);
        assert_eq!(r.reqs, 2);
        assert_eq!(r.c5xx, 1);
        assert_eq!(r.c4xx, 0);
        assert_eq!(r.max_seg, Some(2.0));
    }

    #[test]
    fn prune_remove_somente_o_antigo() {
        let conn = banco();
        inserir_container(&conn, "app", "running", 1000);
        let mut niveis = std::collections::BTreeMap::new();
        niveis.insert("INFO".to_string(), 1usize);
        armazenar_contagens(&conn, "app", &niveis, 100).unwrap();
        armazenar_contagens(&conn, "app", &niveis, 900).unwrap();

        prune_antigos(&conn, 500).unwrap();

        let restantes: i64 = conn
            .query_row("SELECT COUNT(*) FROM log_counts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(restantes, 1);
    }

    #[test]
    fn armazenar_linhas_acumula_entre_coletas() {
        let conn = banco();
        let mut grupos = std::collections::BTreeMap::new();
        grupos.insert("INFO".to_string(), vec!["linha 1".to_string()]);
        armazenar_linhas(&conn, "app", &grupos, 100).unwrap();
        armazenar_linhas(&conn, "app", &grupos, 200).unwrap();

        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM log_lines", [], |r| r.get(0))
            .unwrap();
        // Antes esta função APAGAVA as linhas anteriores; agora acumula
        // (a retenção é por tempo, via prune_antigos).
        assert_eq!(total, 2);
    }
}
```

Nota 1: `LoguruEntry` precisa derivar `Clone` — já deriva (`#[derive(Debug, Clone)]`).

Nota 2: se o teste `resumo_janela_agrega_contagens_e_requests` falhar no `parse_loguru_line(linha).unwrap()`, copie a linha de log usada pelo teste existente `parse_loguru_linha_completa` em `crates/nucleo/src/core.rs` (é uma fixture que comprovadamente parseia) e ajuste as asserções de duração/status conforme os valores dela.

- [ ] **Step 4.4: Rodar testes e clippy**

Run: `cargo test -p nucleo 2>&1 | tail -5`
Expected: todos os testes do nucleo (config + executor + metricas + db + core) passam.

Run: `cargo test --workspace 2>&1 | tail -3 && cargo clippy --workspace -- -D warnings 2>&1 | tail -3`
Expected: ok e limpo. (Se `exibir_estatisticas` no cli reclamar de linhas acumuladas, não afeta: ela lê `log_counts`, que já era acumulativa.)

- [ ] **Step 4.5: Commit**

```bash
git add -A && git commit -m "feat(nucleo): tabela requests, retenção por tempo e métricas de severidade"
```

---

### Task 5: Coletor reutilizável (thread + canais mpsc)

**Files:**
- Create: `crates/nucleo/src/coletor.rs`
- Modify: `crates/nucleo/src/lib.rs` (adicionar `pub mod coletor;`)

**Interfaces:**
- Consumes: `executor::{Executor, listar_containers, obter_logs, obter_logs_desde}` (Task 3), `core::{categorizar_por_nivel, parse_loguru_line}`, `db::{init_db, armazenar_contagens, armazenar_linhas, armazenar_requests, verificar_status_containers, prune_antigos}` (Task 4).
- Produces:
  - `coletor::EventoColeta { Novo, Falha(String) }`
  - `coletor::ComandoColetor { ColetarAgora, Encerrar }`
  - `coletor::ParametrosColetor { executor: Executor, db: PathBuf, intervalo: Duration, tail_inicial: usize, retencao_horas: u64 }` (todos `pub`)
  - `coletor::coletar_ciclo(&Executor, &Connection, tail_inicial: usize, retencao_horas: u64) -> Result<(), Box<dyn Error>>`
  - `coletor::iniciar_coletor(ParametrosColetor, Sender<EventoColeta>) -> (JoinHandle<()>, Sender<ComandoColetor>)`

- [ ] **Step 5.1: Escrever coletor.rs**

Crie `crates/nucleo/src/coletor.rs`:

```rust
// CASCA DE IO: o coletor de logs — o coração "ao vivo" do dashboard.
//
// Duas peças:
//   - `coletar_ciclo`: UM ciclo completo de coleta (docker ps -> alertas ->
//     docker logs incremental -> parse -> SQLite -> prune). Reutilizável:
//     o CLI roda em thread; o futuro dev-server (Fase 2) roda como serviço.
//   - `iniciar_coletor`: sobe a thread que repete o ciclo a cada intervalo
//     e conversa com a TUI por DOIS canais mpsc (eventos para lá, comandos
//     para cá).
//
// Sobre concorrência: `rusqlite::Connection` não é `Sync` (não pode ser
// compartilhada entre threads), então a thread coletora abre a SUA conexão
// e a TUI usa outra. O modo WAL do SQLite permite um escritor e vários
// leitores simultâneos sem bloqueio — exatamente o nosso caso.
// docs: https://www.sqlite.org/wal.html
// docs: https://doc.rust-lang.org/book/ch16-00-concurrency.html

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::core::{categorizar_por_nivel, parse_loguru_line};
use crate::db::{
    armazenar_contagens, armazenar_linhas, armazenar_requests, init_db, prune_antigos,
    verificar_status_containers,
};
use crate::executor::{listar_containers, obter_logs, obter_logs_desde, Executor};

/// O que a thread coletora anuncia para quem estiver ouvindo (a TUI).
#[derive(Debug)]
pub enum EventoColeta {
    /// Um ciclo terminou com sucesso; há dados novos no banco.
    Novo,
    /// O ciclo falhou (docker/ssh fora do ar etc.); tenta de novo no próximo.
    Falha(String),
}

/// O que quem estiver de fora pode pedir à thread coletora.
#[derive(Debug)]
pub enum ComandoColetor {
    /// Executa um ciclo imediatamente (tecla `r` do dashboard).
    ColetarAgora,
    /// Termina a thread de forma limpa.
    Encerrar,
}

/// Parâmetros para subir o coletor (agrupados numa struct para a assinatura
/// de `iniciar_coletor` não virar uma fila de argumentos posicionais).
pub struct ParametrosColetor {
    pub executor: Executor,
    pub db: PathBuf,
    pub intervalo: Duration,
    pub tail_inicial: usize,
    pub retencao_horas: u64,
}

/// Timestamp Unix atual em segundos (0 se o relógio estiver antes de 1970).
fn agora_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// UM ciclo de coleta. Não decide QUANDO rodar — só roda. Reutilizado pelo
/// CLI (em thread) e pelo futuro dev-server.
pub fn coletar_ciclo(
    executor: &Executor,
    conn: &Connection,
    tail_inicial: usize,
    retencao_horas: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let agora = agora_unix();

    // 1. Quem está rodando? (também detecta parados/reiniciados)
    let rodando = listar_containers(executor)?;
    let nomes: Vec<String> = rodando.iter().map(|c| c.nome.clone()).collect();
    // Os alertas ficam gravados na tabela `alerts`; a TUI lê o status
    // 'stopped' direto de `containers`, então aqui só registramos.
    let _ = verificar_status_containers(conn, &nomes, agora)?;

    // 2. Coleta incremental de cada container rodando.
    for c in &rodando {
        let ultima_coleta: i64 = conn
            .query_row(
                "SELECT COALESCE(last_collected_at, 0) FROM containers WHERE name = ?1",
                rusqlite::params![c.nome],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let conteudo = if ultima_coleta == 0 {
            // Primeira vez que vemos este container: pega o rabo do log.
            obter_logs(executor, &c.nome, tail_inicial)?
        } else {
            // Já conhecido: só o que chegou desde a última coleta.
            obter_logs_desde(executor, &c.nome, ultima_coleta)?
        };

        // 3. Parse: linhas por nível + requests HTTP (formato Loguru).
        let grupos = categorizar_por_nivel(&conteudo);
        let niveis: std::collections::BTreeMap<String, usize> =
            grupos.iter().map(|(k, v)| (k.clone(), v.len())).collect();
        let entradas: Vec<_> = conteudo.lines().filter_map(parse_loguru_line).collect();

        // 4. Persiste tudo desta coleta com o MESMO collected_at.
        armazenar_contagens(conn, &c.nome, &niveis, agora)?;
        armazenar_linhas(conn, &c.nome, &grupos, agora)?;
        armazenar_requests(conn, &c.nome, &entradas, agora)?;
        conn.execute(
            "INSERT OR REPLACE INTO containers (name, status, last_collected_at, uptime, criado_em)
             VALUES (?1, 'running', ?2, ?3, ?4)",
            rusqlite::params![c.nome, agora, c.status, c.criado_em],
        )?;
    }

    // 5. Retenção: descarta o que passou da validade.
    let corte = agora - (retencao_horas as i64) * 3600;
    prune_antigos(conn, corte)?;

    Ok(())
}

/// Sobe a thread coletora. Devolve o handle (para `join` na saída) e o
/// sender de comandos (para `ColetarAgora`/`Encerrar`).
pub fn iniciar_coletor(
    parametros: ParametrosColetor,
    eventos: mpsc::Sender<EventoColeta>,
) -> (thread::JoinHandle<()>, mpsc::Sender<ComandoColetor>) {
    let (tx_comandos, rx_comandos) = mpsc::channel::<ComandoColetor>();

    // `move`: a closure toma POSSE de `parametros`, `eventos` e
    // `rx_comandos` — obrigatório em `thread::spawn`, porque a thread pode
    // viver mais que a função que a criou (ownership transferido, não
    // emprestado).
    // docs: https://doc.rust-lang.org/book/ch16-01-threads.html#using-move-closures-with-threads
    let handle = thread::spawn(move || {
        // A conexão é criada DENTRO da thread (Connection não atravessa
        // threads com segurança). Falha ao abrir = avisa e morre; a TUI
        // mostra a falha.
        let conn = match Connection::open(&parametros.db) {
            Ok(c) => c,
            Err(erro) => {
                let _ = eventos.send(EventoColeta::Falha(format!(
                    "não abriu o banco {}: {erro}",
                    parametros.db.display()
                )));
                return;
            }
        };
        // WAL: escritor (esta thread) e leitores (TUI) convivem sem lock.
        // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.pragma_update
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        if let Err(erro) = init_db(&conn) {
            let _ = eventos.send(EventoColeta::Falha(format!("init_db falhou: {erro}")));
            return;
        }

        loop {
            // Roda um ciclo e anuncia o resultado. `let _ =` no send: se o
            // receptor já morreu (TUI fechou), não há o que fazer — o loop
            // termina no `recv_timeout` abaixo (Disconnected).
            match coletar_ciclo(
                &parametros.executor,
                &conn,
                parametros.tail_inicial,
                parametros.retencao_horas,
            ) {
                Ok(()) => {
                    let _ = eventos.send(EventoColeta::Novo);
                }
                Err(erro) => {
                    let _ = eventos.send(EventoColeta::Falha(erro.to_string()));
                }
            }

            // Espera o intervalo OU um comando — o `recv_timeout` faz os
            // dois papéis de uma vez (sleep interrompível).
            // docs: https://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html#method.recv_timeout
            match rx_comandos.recv_timeout(parametros.intervalo) {
                Ok(ComandoColetor::ColetarAgora) => continue,
                Ok(ComandoColetor::Encerrar) => break,
                // Canal fechado = o outro lado (TUI) sumiu; encerra junto.
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                // Timeout = passou o intervalo; próximo ciclo.
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
            }
        }
    });

    (handle, tx_comandos)
}
```

- [ ] **Step 5.2: Registrar o módulo**

Em `crates/nucleo/src/lib.rs`, adicione:

```rust
pub mod coletor;
```

- [ ] **Step 5.3: Compilar, testes, clippy**

`coletar_ciclo`/`iniciar_coletor` são casca de IO (docker/ssh reais) — sem teste unitário; a verificação é manual na Task 8. As partes puras que eles usam já estão testadas (core, metricas, db).

Run: `cargo build --workspace && cargo test --workspace 2>&1 | tail -3 && cargo clippy --workspace -- -D warnings 2>&1 | tail -3`
Expected: tudo ok. (`coletor` compila sem warnings de dead_code porque é `pub` em lib.)

- [ ] **Step 5.4: Commit**

```bash
git add -A && git commit -m "feat(nucleo): coletor reutilizável em thread com canais mpsc"
```

---

### Task 6: Dashboard TUI + coleta ao vivo no loop de eventos

**Files:**
- Create: `crates/cli/src/screens/dashboard.rs`
- Modify: `crates/cli/src/screens/mod.rs` (mod dashboard + método `atualizar` no trait)
- Modify: `crates/cli/src/tui.rs` (poll com timeout + canal de eventos + tela inicial parametrizada)
- Create: `crates/cli/src/logs/dashboard.rs` (subcomando `logs dashboard`)
- Modify: `crates/cli/src/logs/mod.rs` (variante Dashboard; adaptar chamada de run_tui)
- Modify: `crates/cli/src/logs/remote.rs` (adaptar chamada de run_tui)

**Interfaces:**
- Consumes: `nucleo::config::Config` (Task 2), `nucleo::executor::Executor` (Task 3), `nucleo::db::resumo_janela` + `nucleo::metricas::{severidade, Severidade, ResumoContainer}` (Task 4), `nucleo::coletor::{iniciar_coletor, ParametrosColetor, EventoColeta, ComandoColetor}` (Task 5), `AppTypeScreen::new(container: String, linhas: Vec<String>)` e `carregar_todas_linhas(conn, container) -> Vec<String>` (já existem em screens).
- Produces: `dev-cli logs dashboard` funcionando ponta a ponta.

- [ ] **Step 6.1: Adicionar `atualizar` ao trait Screen**

Em `crates/cli/src/screens/mod.rs`:

1. Adicione `pub mod dashboard;` à lista de módulos.
2. Adicione o import no topo: `use nucleo::coletor::EventoColeta;`
3. Dentro de `pub(crate) trait Screen { ... }`, adicione (depois de `handle_click`):

```rust
    /// Reage a um evento da thread coletora (dados novos ou falha).
    /// Implementação padrão: ignora — só o dashboard reage hoje, as telas
    /// de drill-down mostram um recorte estático de quando foram abertas.
    fn atualizar(&mut self, _evento: &EventoColeta, _conn: &Connection) {}
```

- [ ] **Step 6.2: Escrever a tela do dashboard**

Crie `crates/cli/src/screens/dashboard.rs`:

```rust
// Tela inicial da TUI: o dashboard "onde estão os problemas".
//
// Mostra todos os containers ranqueados por severidade (parado > vermelho >
// amarelo > verde), com erros, status HTTP e tempos de resposta da janela
// configurada. A tela NÃO coleta nada: ela lê os agregados do SQLite
// (resumo_janela) e é avisada pela thread coletora via `atualizar`.
//
// Navegação: ↑/↓ seleciona, Enter mergulha no container (drill-down),
// r pede coleta imediata, q sai.

use std::sync::mpsc::Sender;
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
    Frame,
};
use rusqlite::Connection;

use nucleo::coletor::{ComandoColetor, EventoColeta};
use nucleo::config::Limiares;
use nucleo::db::resumo_janela;
use nucleo::metricas::{severidade, ResumoContainer, Severidade};

use crate::screens::app_types::AppTypeScreen;
use crate::screens::lines::carregar_todas_linhas;
use crate::screens::{Screen, ScreenAction};

/// Timestamp Unix atual (segundos).
fn agora_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub(crate) struct DashboardScreen {
    /// Resumos já classificados e ordenados (pior primeiro).
    itens: Vec<(Severidade, ResumoContainer)>,
    selected: usize,
    limiares: Limiares,
    /// Minutos da janela de estatísticas (só para exibir no título).
    janela_min: u64,
    /// Rótulo da origem dos dados: "local" ou "ssh: user@host".
    origem: String,
    /// Momento (unix) da última coleta bem-sucedida vista pela tela.
    ultima_coleta_ok: Option<i64>,
    /// Última falha de coleta (mensagem, quando) — some no próximo sucesso.
    falha: Option<(String, i64)>,
    /// Canal para pedir "coletar agora" à thread coletora (tecla r).
    comandos: Sender<ComandoColetor>,
}

impl DashboardScreen {
    pub(crate) fn new(
        conn: &Connection,
        limiares: Limiares,
        janela_min: u64,
        origem: String,
        comandos: Sender<ComandoColetor>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut tela = Self {
            itens: Vec::new(),
            selected: 0,
            limiares,
            janela_min,
            origem,
            ultima_coleta_ok: None,
            falha: None,
            comandos,
        };
        tela.recarregar(conn)?;
        Ok(tela)
    }

    /// Relê os agregados da janela no banco, classifica e ordena.
    fn recarregar(&mut self, conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
        let corte = agora_unix() - (self.janela_min as i64) * 60;
        let mut itens: Vec<(Severidade, ResumoContainer)> = resumo_janela(conn, corte)?
            .into_iter()
            .map(|r| (severidade(&r, &self.limiares), r))
            .collect();
        // Pior primeiro: ordena por severidade DESC e, dentro dela, por
        // quantidade de problemas DESC. `sort_by` com `cmp` invertido
        // (b antes de a) é o idioma para ordem decrescente.
        // docs: https://doc.rust-lang.org/std/vec/struct.Vec.html#method.sort_by
        itens.sort_by(|a, b| {
            let problemas_a = a.1.erros + a.1.crits + a.1.c5xx;
            let problemas_b = b.1.erros + b.1.crits + b.1.c5xx;
            (b.0, problemas_b).cmp(&(a.0, problemas_a))
        });
        self.itens = itens;
        self.selected = self.selected.min(self.itens.len().saturating_sub(1));
        Ok(())
    }
}

/// Ícone + cor de cada severidade.
fn aparencia(sev: Severidade) -> (&'static str, Color) {
    match sev {
        Severidade::Parado => ("✖", Color::Red),
        Severidade::Vermelho => ("●", Color::Red),
        Severidade::Amarelo => ("●", Color::Yellow),
        Severidade::Verde => ("○", Color::Green),
    }
}

/// Formata `Option<f64>` de segundos como "1.23s" ou "—".
fn fmt_seg(valor: Option<f64>) -> String {
    match valor {
        Some(v) => format!("{v:.2}s"),
        None => "—".to_string(),
    }
}

/// Formata um inteiro, trocando zero por "—" para aliviar a tabela.
fn fmt_n(valor: i64) -> String {
    if valor == 0 {
        "—".to_string()
    } else {
        valor.to_string()
    }
}

impl Screen for DashboardScreen {
    fn handle_key(&mut self, key: KeyCode, conn: &Connection) -> ScreenAction {
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                ScreenAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.itens.len().saturating_sub(1);
                self.selected = self.selected.saturating_add(1).min(max);
                ScreenAction::None
            }
            KeyCode::Enter => {
                if self.itens.is_empty() {
                    return ScreenAction::None;
                }
                let nome = self.itens[self.selected].1.nome.clone();
                let linhas = carregar_todas_linhas(conn, &nome);
                ScreenAction::Push(Box::new(AppTypeScreen::new(nome, linhas)))
            }
            KeyCode::Char('r') => {
                // Pede um ciclo imediato; o resultado chega via `atualizar`.
                let _ = self.comandos.send(ComandoColetor::ColetarAgora);
                ScreenAction::None
            }
            KeyCode::Char('q') | KeyCode::Esc => ScreenAction::Quit,
            _ => ScreenAction::None,
        }
    }

    fn atualizar(&mut self, evento: &EventoColeta, conn: &Connection) {
        match evento {
            EventoColeta::Novo => {
                self.ultima_coleta_ok = Some(agora_unix());
                self.falha = None;
                // Erro ao reler é tratado como falha "de coleta" na UI —
                // melhor mostrar o problema do que derrubar a TUI.
                if let Err(erro) = self.recarregar(conn) {
                    self.falha = Some((erro.to_string(), agora_unix()));
                }
            }
            EventoColeta::Falha(mensagem) => {
                self.falha = Some((mensagem.clone(), agora_unix()));
            }
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // cabeçalho (origem + resumo global)
                Constraint::Min(1),    // tabela
                Constraint::Length(1), // rodapé de teclas
            ])
            .split(f.area());

        // --- Cabeçalho -------------------------------------------------
        let agora = agora_unix();
        let coleta = match (self.ultima_coleta_ok, &self.falha) {
            (_, Some((mensagem, quando))) => format!(
                "⚠ coleta falhou há {}s: {}",
                agora - quando,
                mensagem.lines().next().unwrap_or("")
            ),
            (Some(quando), None) => format!("coleta há {}s", agora - quando),
            (None, None) => "aguardando primeira coleta…".to_string(),
        };
        let problemas = self
            .itens
            .iter()
            .filter(|(sev, _)| *sev >= Severidade::Vermelho)
            .count();
        let total_reqs: i64 = self.itens.iter().map(|(_, r)| r.reqs).sum();
        let total_erros: i64 = self.itens.iter().map(|(_, r)| r.erros + r.crits).sum();
        let cabecalho = format!(
            " dev-cli · {} · {} · janela {}min\n ▍{} problema(s) · {} containers · {} reqs · {} erros",
            self.origem,
            coleta,
            self.janela_min,
            problemas,
            self.itens.len(),
            total_reqs,
            total_erros,
        );
        let estilo_cabecalho = if self.falha.is_some() {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Cyan)
        };
        f.render_widget(Paragraph::new(cabecalho).style(estilo_cabecalho), area[0]);

        // --- Tabela -----------------------------------------------------
        let linhas: Vec<Row> = self
            .itens
            .iter()
            .map(|(sev, r)| {
                let (icone, cor) = aparencia(*sev);
                Row::new(vec![
                    Cell::from(icone).style(Style::default().fg(cor)),
                    Cell::from(r.nome.clone()),
                    Cell::from(r.uptime.clone()),
                    Cell::from(fmt_n(r.erros)),
                    Cell::from(fmt_n(r.crits)),
                    Cell::from(fmt_n(r.c5xx)),
                    Cell::from(fmt_n(r.c4xx)),
                    Cell::from(fmt_seg(r.p95_seg)),
                    Cell::from(fmt_seg(r.max_seg)),
                    Cell::from(fmt_n(r.reqs)),
                ])
                .style(Style::default().fg(cor))
            })
            .collect();

        let tabela = Table::new(
            linhas,
            [
                Constraint::Length(2),  // ícone
                Constraint::Min(20),    // container
                Constraint::Length(16), // uptime
                Constraint::Length(5),  // ERR
                Constraint::Length(5),  // CRIT
                Constraint::Length(5),  // 5xx
                Constraint::Length(5),  // 4xx
                Constraint::Length(8),  // p95
                Constraint::Length(8),  // máx
                Constraint::Length(6),  // reqs
            ],
        )
        .header(
            Row::new(vec![
                "", "CONTAINER", "STATUS", "ERR", "CRIT", "5xx", "4xx", "p95", "máx", "reqs",
            ])
            .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(Block::default().borders(Borders::ALL).title(" Containers "))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        // `TableState` guarda a seleção; o ratatui rola a tabela sozinho
        // para manter a linha selecionada visível.
        // docs: https://docs.rs/ratatui/latest/ratatui/widgets/struct.TableState.html
        let mut estado = TableState::default();
        estado.select(Some(self.selected));
        f.render_stateful_widget(tabela, area[1], &mut estado);

        // --- Rodapé -----------------------------------------------------
        let ajuda = Paragraph::new("  ↑/↓ navegar · Enter detalhes · r coletar agora · q sair")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(ajuda, area[2]);
    }
}
```

- [ ] **Step 6.3: Adaptar o loop de eventos em tui.rs**

Substitua TODO o conteúdo de `crates/cli/src/tui.rs` por:

```rust
// Módulo do TUI (Terminal User Interface).
//
// ORQUESTRAÇÃO: este arquivo só gerencia o terminal (raw mode, tela
// alternada) e a pilha de telas; a lógica de cada tela vive em `screens/`.
//
// O loop principal mudou para suportar coleta AO VIVO: em vez do
// `event::read()` bloqueante (que só acorda com tecla), usamos
// `event::poll(250ms)` — a cada 250ms sem tecla o loop dá uma volta,
// drena o canal de eventos da thread coletora e redesenha. Assim o
// dashboard atualiza sozinho e o relógio "coleta há Xs" anda.
//
// docs: https://docs.rs/ratatui/latest/ratatui/
// docs: https://docs.rs/crossterm/latest/crossterm/event/fn.poll.html

use std::sync::mpsc::Receiver;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use nucleo::coletor::EventoColeta;
use rusqlite::Connection;

use crate::screens::{Screen, ScreenAction};

/// Ponto de entrada da TUI. `tela_inicial` define o que abre primeiro
/// (dashboard ao vivo ou drill-down estático); `eventos` é o canal da
/// thread coletora (None = TUI estática, sem coleta ao vivo).
pub(crate) fn run_tui(
    conn: &Connection,
    tela_inicial: Box<dyn Screen>,
    eventos: Option<Receiver<EventoColeta>>,
) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // Pilha de telas: Enter empilha uma tela-filha; Esc/Backspace desempilha.
    let mut screens: Vec<Box<dyn Screen>> = vec![tela_inicial];

    let res = loop {
        terminal.draw(|f| {
            if let Some(screen) = screens.last_mut() {
                screen.draw(f);
            }
        })?;

        // 1. Entrega à tela do topo os eventos da coleta que chegaram.
        // `try_recv` não bloqueia: drena o que houver e segue.
        // docs: https://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html#method.try_recv
        if let Some(rx) = &eventos {
            while let Ok(evento) = rx.try_recv() {
                if let Some(screen) = screens.last_mut() {
                    screen.atualizar(&evento, conn);
                }
            }
        }

        // 2. Espera tecla/mouse por até 250ms; sem nada, volta ao draw
        // (é isso que faz o dashboard "andar" sem o usuário digitar).
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        let action = match event::read()? {
            Event::Key(key) => screens
                .last_mut()
                .map(|s| s.handle_key(key.code, conn))
                .unwrap_or(ScreenAction::Quit),
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => screens
                    .last_mut()
                    .map(|s| s.handle_key(KeyCode::Down, conn))
                    .unwrap_or(ScreenAction::None),
                MouseEventKind::ScrollUp => screens
                    .last_mut()
                    .map(|s| s.handle_key(KeyCode::Up, conn))
                    .unwrap_or(ScreenAction::None),
                MouseEventKind::Down(_) => screens
                    .last_mut()
                    .map(|s| s.handle_click(mouse.row, mouse.column, conn))
                    .unwrap_or(ScreenAction::None),
                _ => ScreenAction::None,
            },
            _ => ScreenAction::None,
        };

        match action {
            ScreenAction::Push(s) => screens.push(s),
            ScreenAction::Pop => {
                screens.pop();
                if screens.is_empty() {
                    break Ok(());
                }
            }
            ScreenAction::Quit => break Ok(()),
            ScreenAction::None => {}
        }
    };

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    res
}
```

- [ ] **Step 6.4: Adaptar os chamadores antigos de run_tui**

Em `crates/cli/src/logs/mod.rs`, a chamada `crate::tui::run_tui(&conn)?;` vira:

```rust
            crate::tui::run_tui(
                &conn,
                Box::new(crate::screens::containers::ContainerScreen::new(&conn)?),
                None,
            )?;
```

Em `crates/cli/src/logs/remote.rs`, a chamada `crate::tui::run_tui(&conn)?;` vira o mesmo bloco acima. Em `crates/cli/src/tui.rs` o import de `ContainerScreen` foi removido no Step 6.3 — confirme que `screens/containers.rs` continua compilando (é usado pelos dois chamadores acima até a Task 7).

- [ ] **Step 6.5: Criar o subcomando `logs dashboard`**

Crie `crates/cli/src/logs/dashboard.rs`:

```rust
// CASCA DE IO: subcomando `logs dashboard` — o modo "ao vivo" do dev-cli.
// Resolve a configuração (flags > env > arquivo > defaults), sobe a thread
// coletora do nucleo e entrega o terminal ao dashboard.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use clap::Args;
use rusqlite::Connection;

use nucleo::coletor::{iniciar_coletor, ComandoColetor, ParametrosColetor};
use nucleo::config::Config;
use nucleo::db::init_db;
use nucleo::executor::Executor;

use crate::screens::dashboard::DashboardScreen;

/// Dashboard ao vivo: coleta contínua + visão de problemas por container.
#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct DashboardArgs {
    /// Caminho do arquivo de configuração TOML.
    /// (default: ~/.config/dev-cli/config.toml, se existir)
    #[arg(long)]
    config: Option<PathBuf>,
    /// Host SSH ("user@host") para coletar de uma VM remota.
    /// Sem esta flag, executa `docker` localmente (modo padrão na VM).
    #[arg(long)]
    ssh: Option<String>,
    /// Caminho do banco SQLite (sobrepõe config/env).
    #[arg(long)]
    db: Option<PathBuf>,
    /// Segundos entre coletas (sobrepõe config/env).
    #[arg(long)]
    intervalo: Option<u64>,
}

impl DashboardArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // 1. Config com precedência: flags (aqui) > env > arquivo > defaults.
        // `as_deref()`: converte `&Option<PathBuf>` em `Option<&Path>` sem
        // clonar — o idioma para passar "talvez um caminho" por referência.
        // docs: https://doc.rust-lang.org/std/option/enum.Option.html#method.as_deref
        let mut config = Config::carregar(self.config.as_deref())?;
        if let Some(ssh) = &self.ssh {
            config.coleta.ssh = ssh.clone();
        }
        if let Some(db) = &self.db {
            config.coleta.db = db.display().to_string();
        }
        if let Some(intervalo) = self.intervalo {
            config.coleta.intervalo_seg = intervalo;
        }

        let executor = if config.coleta.ssh.is_empty() {
            Executor::Local
        } else {
            Executor::Ssh(config.coleta.ssh.clone())
        };
        let origem = if config.coleta.ssh.is_empty() {
            "local".to_string()
        } else {
            format!("ssh: {}", config.coleta.ssh)
        };

        // 2. Banco: garante o diretório e o schema ANTES de subir a thread
        // (as duas conexões — TUI e coletor — apontam para o mesmo arquivo).
        let caminho_db = config.caminho_db();
        if let Some(pai) = caminho_db.parent() {
            std::fs::create_dir_all(pai)?;
        }
        let conn = Connection::open(&caminho_db)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        init_db(&conn)?;

        // 3. Sobe o coletor e abre a TUI com o canal de eventos.
        let (tx_eventos, rx_eventos) = mpsc::channel();
        let (handle, tx_comandos) = iniciar_coletor(
            ParametrosColetor {
                executor,
                db: caminho_db,
                intervalo: Duration::from_secs(config.coleta.intervalo_seg),
                tail_inicial: config.coleta.tail_inicial,
                retencao_horas: config.coleta.retencao_horas,
            },
            tx_eventos,
        );

        let tela = DashboardScreen::new(
            &conn,
            config.limiares.clone(),
            config.coleta.janela_min,
            origem,
            tx_comandos.clone(),
        )?;
        let resultado = crate::tui::run_tui(&conn, Box::new(tela), Some(rx_eventos));

        // 4. Encerramento limpo: pede para a thread parar e espera.
        // (Se um ciclo estiver no meio de um `docker logs`, o join espera
        // ele terminar — aceitável para um comando interativo.)
        let _ = tx_comandos.send(ComandoColetor::Encerrar);
        let _ = handle.join();

        resultado?;
        Ok(String::new())
    }
}
```

- [ ] **Step 6.6: Registrar o subcomando**

Em `crates/cli/src/logs/mod.rs`:

1. Adicione `mod dashboard;` junto aos outros `mod`.
2. Adicione a variante no enum `LogsCommands` (primeira posição):

```rust
    /// Dashboard ao vivo: onde estão os problemas nos containers.
    Dashboard(dashboard::DashboardArgs),
```

3. Adicione o braço no `match` de `execute()`:

```rust
            Some(LogsCommands::Dashboard(args)) => args.execute(),
```

- [ ] **Step 6.7: Compilar, testes, clippy**

Run: `cargo build --workspace && cargo test --workspace 2>&1 | tail -3 && cargo clippy --workspace -- -D warnings 2>&1 | tail -3`
Expected: tudo verde.

Run: `cargo run -q -p dev-cli -- logs dashboard --help`
Expected: mostra as opções `--config`, `--ssh`, `--db`, `--intervalo`.

- [ ] **Step 6.8: Commit**

```bash
git add -A && git commit -m "feat(cli): dashboard TUI ao vivo com coleta em thread e ranking de severidade"
```

---

### Task 7: Limpeza — remover caminhos substituídos pelo dashboard

**Files:**
- Modify: `crates/cli/src/logs/mod.rs` (remover `--tui`/`--db` de LogsArgs)
- Modify: `crates/cli/src/logs/remote.rs` (remover `--tui`)
- Delete: `crates/cli/src/screens/containers.rs`
- Modify: `crates/cli/src/screens/mod.rs` (remover `pub mod containers;`)
- Modify: `crates/cli/src/tui.rs` (nada — já não referencia ContainerScreen)

- [ ] **Step 7.1: Remover o modo --tui de LogsArgs**

Em `crates/cli/src/logs/mod.rs`, na struct `LogsArgs`:
1. DELETE os campos `tui: bool` e `db: Option<PathBuf>` (e seus comentários/atributos `#[arg]`).
2. DELETE o import `use std::path::PathBuf;`.
3. No `execute()`, DELETE o bloco inteiro `if self.tui { ... return Ok(String::new()); }` (o modo TUI agora é `logs dashboard`).
4. O campo `comando: Option<LogsCommands>` volta a ser obrigatório: troque para `comando: LogsCommands` (sem `Option`), e o `match &self.comando` perde os `Some(...)` e o braço `None`:

```rust
        match &self.comando {
            LogsCommands::Dashboard(args) => args.execute(),
            LogsCommands::Stats(args) => args.execute(),
            LogsCommands::Containers(args) => args.execute(),
            LogsCommands::Remote(args) => args.execute(),
        }
```

- [ ] **Step 7.2: Remover o modo --tui do remote**

Em `crates/cli/src/logs/remote.rs`:
1. DELETE o campo `tui: bool` da struct.
2. No `execute()`, DELETE o bloco inteiro `let db_vazio = conn.query_row(...) ... == 0;` (com seus comentários — ele só existia para o modo TUI) e DELETE a chamada final a `run_tui` com seu comentário.
3. Sem `--tui`, o caminho `--watch` precisa de um `loop` explícito (hoje ele coleta uma vez, dorme e caía na TUI). Reestruture o final do método assim (substituindo o trecho desde `if db_vazio || !self.tui {` até o fim do método):

```rust
        // Coleta + exibição; com --watch, repete a cada 5 minutos.
        loop {
            let agora = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let mut saida = String::new();

            // 1. Descobre containers rodando (ou usa o específico).
            let rodando = if let Some(nome) = &self.container {
                vec![ContainerDocker {
                    nome: nome.clone(),
                    status: String::new(),
                    criado_em: String::new(),
                }]
            } else {
                listar_containers(&executor)?
            };
            let nomes_rodando: Vec<String> = rodando.iter().map(|c| c.nome.clone()).collect();
            let alertas = verificar_status_containers(&conn, &nomes_rodando, agora)?;
            for alerta in &alertas {
                saida.push_str(&format!("⚠️  {}\n", alerta.bold()));
            }

            // 2. Coleta incremental dos que estão rodando.
            for c in &rodando {
                let ultima_coleta: i64 = conn
                    .query_row(
                        "SELECT COALESCE(last_collected_at, 0) FROM containers WHERE name = ?1",
                        rusqlite::params![c.nome],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                let conteudo = if ultima_coleta == 0 {
                    obter_logs(&executor, &c.nome, self.tail)?
                } else {
                    obter_logs_desde(&executor, &c.nome, ultima_coleta)?
                };

                let grupos = categorizar_por_nivel(&conteudo);
                let niveis: std::collections::BTreeMap<String, usize> =
                    grupos.iter().map(|(k, v)| (k.clone(), v.len())).collect();

                armazenar_contagens(&conn, &c.nome, &niveis, agora)?;
                armazenar_linhas(&conn, &c.nome, &grupos, agora)?;
                conn.execute(
                    "INSERT OR REPLACE INTO containers (name, status, last_collected_at, uptime, criado_em) VALUES (?1, 'running', ?2, ?3, ?4)",
                    rusqlite::params![c.nome, agora, c.status, c.criado_em],
                )?;
            }

            // 3. Exibe o acumulado do banco.
            saida.push_str(&exibir_estatisticas(&conn)?);

            if !self.watch {
                return Ok(saida.trim_end().to_string());
            }

            // Modo --watch: redesenha o painel e dorme 5 minutos.
            print!("\x1b[2J\x1b[H{}", saida.trim_end());
            std::io::stdout().flush()?;
            std::thread::sleep(Duration::from_secs(300));
        }
```

Mantenha os comentários didáticos originais dos trechos preservados. A checagem `db_vazio` some (era só para o modo TUI). Remova imports que ficarem sem uso (o compilador aponta).

- [ ] **Step 7.3: Deletar a tela ContainerScreen**

```bash
git rm crates/cli/src/screens/containers.rs
```

Em `crates/cli/src/screens/mod.rs`, DELETE a linha `pub mod containers;`.

Em `crates/cli/src/logs/mod.rs` e `crates/cli/src/logs/remote.rs`, as chamadas a `run_tui(...ContainerScreen...)` da Task 6.4 devem ter sido removidas nos steps 7.1/7.2 — confirme com:

Run: `grep -rn "ContainerScreen" crates/`
Expected: nenhuma ocorrência.

- [ ] **Step 7.4: Compilar, testes, clippy (o compilador guia a limpeza)**

Run: `cargo build --workspace 2>&1 | tail -10`

Corrija os erros de "unused import"/"cannot find" que a remoção causou (ex.: `use std::path::PathBuf;` em mod.rs, `rusqlite::Connection` importado sem uso etc.). NÃO mude lógica.

Run: `cargo test --workspace 2>&1 | tail -3 && cargo clippy --workspace -- -D warnings 2>&1 | tail -3`
Expected: tudo verde.

Run: `cargo run -q -p dev-cli -- logs --help`
Expected: subcomandos `dashboard`, `stats`, `containers`, `remote`; SEM opção `--tui`.

- [ ] **Step 7.5: Commit**

```bash
git add -A && git commit -m "refactor(cli): dashboard substitui os modos --tui de logs e logs remote"
```

---

### Task 8: Documentação + verificação ponta a ponta

**Files:**
- Modify: `CLAUDE.md` (estrutura workspace, comandos novos)
- Modify: `README.md` (se citar comandos/estrutura antigos)

- [ ] **Step 8.1: Atualizar CLAUDE.md**

Em `CLAUDE.md`:
1. Na seção **Projeto**, troque "binário único, sem workspace" por uma frase descrevendo o workspace (`crates/nucleo` lib + `crates/cli` bin `dev-cli`; `crates/servidor` axum previsto para a Fase 2).
2. Na seção **Comandos**, adicione:

```bash
cargo run -p dev-cli -- logs dashboard              # dashboard TUI ao vivo (docker local)
cargo run -p dev-cli -- logs dashboard --ssh user@host   # idem, coletando via SSH
cargo test --workspace      # roda a suíte inteira
cargo clippy --workspace    # clippy no workspace inteiro
```

3. Na seção **Arquitetura**, descreva a divisão: `crates/nucleo` (core puro: parse/métricas; casca: config/executor/coletor/db) e `crates/cli` (clap + telas ratatui em `src/screens/`, dashboard como tela inicial). Atualize os caminhos citados (`src/logs.rs` → `crates/nucleo/src/core.rs` etc.).
4. Em **Convenções**, mantenha tudo; ajuste o item "Novo subcomando" para citar `crates/cli/src/cli.rs`.

- [ ] **Step 8.2: Atualizar README.md se necessário**

Run: `grep -n "logs remote\|--tui\|src/logs" README.md`
Se houver ocorrências, atualize-as para os comandos/caminhos novos. Se não houver, siga adiante.

- [ ] **Step 8.3: Verificação completa**

Run: `cargo build --workspace && cargo test --workspace 2>&1 | tail -3 && cargo clippy --workspace -- -D warnings 2>&1 | tail -3`
Expected: build ok, todos os testes passam, clippy limpo.

Verificação funcional (requer acesso ao ambiente; executar as que forem possíveis):

1. **Dashboard via SSH (dev):** `cargo run -q -p dev-cli -- logs dashboard --ssh jarede.silva@qa.bistek.com.br --db /tmp/qa-dash.db --intervalo 30`
   - Abre com "aguardando primeira coleta…", popula em segundos.
   - "coleta há Xs" zera a cada ciclo; `r` força ciclo imediato.
   - Containers com erros/5xx no topo, em vermelho; p95/máx preenchidos para apps com linhas Loguru (ex.: qa-prezzo-1).
   - Enter mergulha no drill-down (apps → níveis → linhas); Esc volta; q sai restaurando o terminal.
2. **Falha de coleta não derruba:** `cargo run -q -p dev-cli -- logs dashboard --ssh host-que-nao-existe --db /tmp/falha.db --intervalo 10` — dashboard abre e mostra "⚠ coleta falhou há Xs: ..." no cabeçalho em vermelho; q sai normalmente.
3. **Precedência de config:** `DEV_CLI_COLETA_INTERVALO_SEG=5 cargo run -q -p dev-cli -- logs dashboard --ssh ... --db /tmp/qa-dash.db` coleta a cada 5s; adicionar `--intervalo 60` volta para 60s (flag > env).
4. **Modo one-shot continua:** `cargo run -q -p dev-cli -- logs remote --ssh jarede.silva@qa.bistek.com.br` imprime o painel colorido por container e termina.

- [ ] **Step 8.4: Commit final**

```bash
git add -A && git commit -m "docs: atualiza CLAUDE.md e README para o workspace e o dashboard"
```

---

## Fora deste plano (fases seguintes)

- **Fase 2:** `crates/servidor` — bin `dev-server` com axum, expondo os agregados via HTTP (reusa `nucleo::coletor` como serviço residente); `deploy/` com unit systemd + instalação em RHEL para usuário comum no grupo `docker`.
- **Fase 3:** portal web React + Vite em `web/`, consumindo a API.
