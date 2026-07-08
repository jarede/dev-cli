// Grupo de subcomandos `logs stats`, `logs containers` e `logs remote`,
// além do modo `--tui` para navegação visual em banco SQLite local.
//
// A ideia central de arquitetura aqui é separar três responsabilidades:
//   1. NÚCLEO PURO  -> `core.rs`: funções que recebem texto e devolvem
//      contagens/categorizações. Não tocam em disco nem imprimem nada.
//   2. CASCA DE IO  -> cada subcomando no seu próprio arquivo: descobre
//      arquivos, lê do disco, executa SSH, acessa SQLite e formata a saída.
//   3. RENDERIZAÇÃO -> `render.rs`: monta o texto colorido a partir das
//      contagens, sem saber de onde elas vieram.
// Manter cálculo, IO e apresentação separados facilita testar e raciocinar.

// Cada `mod` abaixo declara um arquivo irmão dentro de `src/logs/`. Sem o
// `pub`, o módulo só é visível dentro de `src/logs/` (detalhes internos de
// implementação).
mod containers;
pub(crate) mod core;
mod db;
mod remote;
mod render;
mod stats;

// `Args`/`Subcommand`: macros de derive do clap que geram, a partir dos
// campos/variantes anotados, o parser de linha de comando (flags, posicionais,
// valores default, help text) sem precisarmos escrever isso à mão.
// docs: https://docs.rs/clap/latest/clap/trait.Args.html
// docs: https://docs.rs/clap/latest/clap/trait.Subcommand.html
use clap::Args;
use clap::Subcommand;
use std::path::PathBuf;

/// Comandos de log.
// `#[derive(Args, Debug)]`: `Args` faz o clap tratar esta struct como um
// grupo de argumentos (aqui, apenas o subcomando aninhado); `Debug` gera
// automaticamente a impressão `{:?}`, útil para inspecionar em depuração.
// `#[command(help_template = ...)]` troca o texto de ajuda padrão do clap
// pelo template compartilhado definido em `crate::help`.
// docs: https://docs.rs/clap/latest/clap/trait.Args.html
// docs: https://doc.rust-lang.org/std/fmt/trait.Debug.html
// docs: https://docs.rs/clap/latest/clap/_derive/index.html
#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS_SUBCOMANDOS)]
pub struct LogsArgs {
    // `logs` é um grupo: ele encaminha para um subcomando aninhado ou ativa
    // o modo TUI. O `Option` permite que o usuário digite apenas `logs --tui`
    // sem um subcomando, mas ainda exige um subcomando nos demais casos.
    #[command(subcommand)]
    comando: Option<LogsCommands>,

    /// Abre TUI interativo para navegar nas estatísticas do banco.
    #[arg(long, help_heading = crate::help::OPCOES)]
    tui: bool,

    /// Caminho do banco SQLite (obrigatório com --tui).
    #[arg(long, help_heading = crate::help::OPCOES)]
    db: Option<PathBuf>,
}

impl LogsArgs {
    // `&self` = empréstimo imutável; só lemos os campos, não os consumimos.
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Modo TUI: abre a interface interativa com o banco local
        if self.tui {
            let db_path = self
                .db
                .as_ref()
                .ok_or("--db é obrigatório com --tui")?;
            let conn = rusqlite::Connection::open(db_path)?;
            // Garante que as tabelas do app existam (container, log_counts,
            // log_lines, alerts) — se o banco já tiver sido criado por
            // `logs remote --db`, o `IF NOT EXISTS` é idempotente.
            // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.execute_batch
            db::init_db(&conn)?;
            crate::tui::run_tui(&conn)?;
            return Ok(String::new());
        }

        // Modo normal: delega para o subcomando escolhido.
        match &self.comando {
            Some(LogsCommands::Stats(args)) => args.execute(),
            Some(LogsCommands::Containers(args)) => args.execute(),
            Some(LogsCommands::Remote(args)) => args.execute(),
            None => Err(
                "nenhum subcomando especificado. Use `logs --help` para ver os subcomandos disponíveis."
                    .into(),
            ),
        }
    }
}

/// Subcomandos de `logs`.
// `#[derive(Subcommand, Debug)]`: `Subcommand` faz o clap gerar o parser que
// decide qual variante (e portanto qual `*Args`) instanciar a partir da
// palavra digitada pelo usuário (`stats`, `containers` ou `remote`).
// docs: https://docs.rs/clap/latest/clap/trait.Subcommand.html
// docs: https://doc.rust-lang.org/std/fmt/trait.Debug.html
#[derive(Subcommand, Debug)]
enum LogsCommands {
    /// Estatísticas de logs de containers (arquivos supervisord).
    Stats(stats::StatsArgs),
    /// Estatísticas de logs dos containers detectados via `container list`.
    Containers(containers::ContainersArgs),
    /// Estatísticas de logs de containers via SSH (docker logs remoto).
    Remote(remote::RemoteArgs),
}
