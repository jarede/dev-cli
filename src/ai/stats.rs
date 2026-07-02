// Subcomando `ai stats`: apenas encaminha para o provedor escolhido. Mesmo
// padrão de encapsulamento de `LogsArgs`/`LogsCommands` em `src/logs.rs`.
use clap::Args;
use clap::Subcommand;

use crate::ai::claude::ClaudeArgs;
use crate::ai::opencode::OpencodeArgs;

/// Estatísticas de um provedor de IA.
#[derive(Args, Debug)]
#[command(help_template = crate::help::SUBCOMANDOS)]
pub struct StatsArgs {
    #[command(subcommand)]
    comando: StatsCommands,
}

impl StatsArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // `match` sobre a referência do enum. Cada variante carrega um tipo de
        // args diferente (`OpencodeArgs` vs `ClaudeArgs`, com campos próprios).
        // Ao adicionar um novo provedor (ex: `Codex(CodexArgs)`), o compilador
        // obriga a tratá-lo aqui (match exaustivo).
        match &self.comando {
            StatsCommands::Opencode(args) => args.execute(),
            StatsCommands::Claude(args) => args.execute(),
        }
    }
}

// Enum dos provedores de IA para o subcomando `ai stats`. Cada variante
// carrega seu próprio `*Args` (e portanto seus campos e defaults).
// Padrão: adicionar um novo provedor é só uma nova variante aqui + um novo
// braço no `match` de `execute()`, com o compilador garantindo que nenhum
// `match` fica incompleto (exaustividade).
/// Provedores de IA disponíveis para estatísticas.
#[derive(Subcommand, Debug)]
enum StatsCommands {
    /// Estatísticas do OpenCode (tokens, custo, heatmap).
    Opencode(OpencodeArgs),
    /// Estatísticas do Claude Code (horas, custo, heatmap).
    Claude(ClaudeArgs),
}
