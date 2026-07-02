// Grupo de subcomandos `ai`. Hoje só tem `stats`, mas a estrutura já
// comporta crescer (ex: `ai chat`) sem precisar migrar nada.
use clap::Args;
use clap::Subcommand;

use crate::ai::stats::StatsArgs;

mod cambio;
mod claude;
mod opencode;
mod precos;
pub mod render;
pub mod stats;

/// Comandos de inteligência artificial.
#[derive(Args, Debug)]
#[command(help_template = crate::help::SUBCOMANDOS)]
pub struct AiArgs {
    #[command(subcommand)]
    comando: AiCommands,
}

impl AiArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // `match` sobre a referência do enum; hoje só há uma variante, mas o
        // `match` deixa o compilador exigir que novas variantes sejam tratadas
        // (match exaustivo). `&self.comando` toma um empréstimo em vez de consumir.
        match &self.comando {
            AiCommands::Stats(args) => args.execute(),
        }
    }
}

#[derive(Subcommand, Debug)]
enum AiCommands {
    /// Estatísticas de uso de ferramentas de IA.
    Stats(StatsArgs),
}
