// Grupo de subcomandos `logs stats`, `logs containers` e `logs remote`.
//
// A ideia central de arquitetura aqui é separar três responsabilidades:
//   1. NÚCLEO PURO  -> `core.rs`: funções que recebem texto e devolvem
//      contagens/categorizações. Não tocam em disco nem imprimem nada.
//   2. CASCA DE IO  -> cada subcomando no seu próprio arquivo: descobre
//      arquivos, lê do disco, executa SSH, acessa SQLite e formata a saída.
//   3. RENDERIZAÇÃO -> `render.rs`: monta o texto colorido a partir das
//      contagens, sem saber de onde elas vieram.
// Manter cálculo, IO e apresentação separados facilita testar e raciocinar.

mod containers;
mod core;
mod db;
mod remote;
mod render;
mod stats;

use clap::Args;
use clap::Subcommand;

/// Comandos de log.
#[derive(Args, Debug)]
#[command(help_template = crate::help::SUBCOMANDOS)]
pub struct LogsArgs {
    #[command(subcommand)]
    comando: LogsCommands,
}

impl LogsArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        match &self.comando {
            LogsCommands::Stats(args) => args.execute(),
            LogsCommands::Containers(args) => args.execute(),
            LogsCommands::Remote(args) => args.execute(),
        }
    }
}

#[derive(Subcommand, Debug)]
enum LogsCommands {
    /// Estatísticas de logs de containers (arquivos supervisord).
    Stats(stats::StatsArgs),
    /// Estatísticas de logs dos containers detectados via `container list`.
    Containers(containers::ContainersArgs),
    /// Estatísticas de logs de containers via SSH (docker logs remoto).
    Remote(remote::RemoteArgs),
}
