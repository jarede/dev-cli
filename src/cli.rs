// Definição da interface de linha de comando (parsing dos argumentos) e o
// despacho para o `execute()` de cada subcomando.

use clap::Parser;

use clap::Args;
use clap::Subcommand;

use crate::ai::AiArgs;
use crate::logs::LogsArgs;

/// Exibe a versão do dev-cli.
#[derive(Args, Debug)]
pub struct VersionArgs;

impl VersionArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // `env!` lê uma variável em tempo de COMPILAÇÃO; `CARGO_PKG_VERSION`
        // vem do `version` do Cargo.toml. Nunca falha em runtime.
        Ok(format!("dev-cli {}", env!("CARGO_PKG_VERSION")))
    }
}

/// Todos os subcomandos de topo. Cada variante carrega os args do seu comando.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Exibe a versão do dev-cli.
    Version(VersionArgs),
    /// Estatísticas de logs de containers (supervisord).
    Logs(LogsArgs),
    /// Comandos de IA (estatísticas de uso e custo).
    Ai(AiArgs),
}

impl Commands {
    // Despacha para o subcomando escolhido. Ao adicionar uma variante nova
    // acima, o compilador obriga a tratá-la aqui (match exaustivo).
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        match self {
            Commands::Version(args) => args.execute(),
            Commands::Logs(args) => args.execute(),
            Commands::Ai(args) => args.execute(),
        }
    }
}

/// dev-cli: canivete suíço para tarefas de desenvolvimento.
#[derive(Parser, Debug)]
#[command(help_template = crate::help::SUBCOMANDOS)]
pub struct Cli {
    // O subcomando escolhido pelo usuário (`version` ou `logs ...`).
    #[command(subcommand)]
    pub command: Commands,
}
