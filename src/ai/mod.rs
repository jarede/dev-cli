// Grupo de subcomandos `ai`. Hoje só tem `stats`, mas a estrutura já
// comporta crescer (ex: `ai chat`) sem precisar migrar nada.
//
// Este arquivo se chama `mod.rs` dentro da pasta `src/ai/`: é a forma
// "clássica" (pré-2018) de declarar o módulo `ai` a partir de uma pasta —
// equivalente a um arquivo `src/ai.rs` que também declarasse `mod cambio;`
// etc., mas usando `mod.rs` como o "arquivo principal" da pasta. É esse
// arquivo que `main.rs` enxerga quando escreve `mod ai;`.
use clap::Args;
use clap::Subcommand;

use crate::ai::stats::StatsArgs;

// Cada `mod` abaixo declara um arquivo irmão dentro de `src/ai/`
// (`cambio.rs`, `claude.rs`, `opencode.rs`, `precos.rs`, `render.rs`,
// `stats.rs`). Sem o `pub`, o módulo só é visível dentro de `src/ai/` (é o
// caso de `cambio`, `claude`, `opencode` e `precos` — detalhes internos de
// implementação); com `pub`, ele também pode ser referenciado de fora deste
// módulo (`render` e `stats`, que outras partes do crate — como `logs.rs` ou
// testes — podem precisar importar diretamente).
mod cambio;
mod claude;
mod opencode;
mod precos;
pub mod render;
pub mod stats;

/// Comandos de inteligência artificial.
// Mesmo padrão de `LogsArgs` em `src/logs.rs`: `AiArgs` é um "grupo" que só
// encaminha para um subcomando aninhado (`comando`), sem argumentos
// próprios. `#[command(help_template = ...)]` usa o mesmo template
// compartilhado de `crate::help` para manter o `--help` consistente entre
// `dev-cli ai --help`, `dev-cli logs --help` etc.
#[derive(Args, Debug)]
#[command(help_template = crate::help::SUBCOMANDOS)]
pub struct AiArgs {
    // `#[command(subcommand)]`: delega ao clap decidir qual variante de
    // `AiCommands` construir a partir da próxima palavra digitada (hoje, só
    // `stats`).
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

// `enum` sem `pub`: só é usado dentro deste módulo (por `AiArgs`), então não
// precisa ser visível para o resto do crate — diferente de `Commands`, em
// `cli.rs`, que é `pub` porque `main.rs` acessa `cli.command` de fora.
#[derive(Subcommand, Debug)]
enum AiCommands {
    /// Estatísticas de uso de ferramentas de IA.
    Stats(StatsArgs),
}
