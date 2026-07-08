// Definição da interface de linha de comando (parsing dos argumentos) e o
// despacho para o `execute()` de cada subcomando.

// `Parser` é a trait/derive de mais alto nível do clap: aplicada na struct
// raiz (`Cli`, lá embaixo), ela gera o `fn parse()` que lê
// `std::env::args()` e devolve a struct já preenchida (ou imprime erro/help
// e encerra o processo, como comentado em `main.rs`).
// docs: https://docs.rs/clap/latest/clap/trait.Parser.html
use clap::Parser;

// `Args` é o derive para "um grupo de argumentos" (usado em structs, aqui em
// `VersionArgs`); `Subcommand` é o derive para "um menu de escolhas" (usado
// em enums, aqui em `Commands`). Um `#[command(subcommand)]` sempre aponta
// para um tipo que implementa `Subcommand`.
// docs: https://docs.rs/clap/latest/clap/trait.Args.html
// docs: https://docs.rs/clap/latest/clap/trait.Subcommand.html
use clap::Args;
use clap::Subcommand;

use crate::ai::AiArgs;
use crate::logs::LogsArgs;

/// Exibe a versão do dev-cli.
// `#[derive(Args, Debug)]` numa struct SEM campos: o clap simplesmente não
// espera nenhum argumento extra depois de `version` — a struct existe só
// para caber no padrão "toda variante de `Commands` carrega um `*Args` com
// `execute()`", mesmo quando não há nada para o usuário configurar.
// docs: https://docs.rs/clap/latest/clap/_derive/index.html
#[derive(Args, Debug)]
pub struct VersionArgs;

impl VersionArgs {
    // `&self`: como a struct não tem campos, o método nem precisa dos dados
    // de `self` de fato, mas mantém a mesma assinatura `fn execute(&self)`
    // que todo `*Args` do projeto segue, o que permite o `match` uniforme em
    // `Commands::execute` logo abaixo.
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // `env!` lê uma variável em tempo de COMPILAÇÃO; `CARGO_PKG_VERSION`
        // vem do `version` do Cargo.toml. Nunca falha em runtime.
        // docs: https://doc.rust-lang.org/std/macro.env.html
        Ok(format!("dev-cli {}", env!("CARGO_PKG_VERSION")))
    }
}

/// Todos os subcomandos de topo. Cada variante carrega os args do seu comando.
// Cada variante é do tipo "tuple variant com um campo": `Version(VersionArgs)`
// guarda uma instância de `VersionArgs` dentro dela. O clap usa o nome da
// variante (em minúsculo/kebab-case) como a palavra digitada na linha de
// comando — por isso `dev-cli version`, `dev-cli logs ...`, `dev-cli ai ...`.
// O `///` de cada variante vira o texto de ajuda daquele subcomando.
// docs: https://docs.rs/clap/latest/clap/trait.Subcommand.html
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
    // acima, o compilador obriga a tratá-la aqui (match exaustivo) — é o
    // clássico caso onde o compilador barra "esquecer" de tratar um novo
    // subcomando: sem o braço correspondente, `cargo build` nem compila.
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // `self` é `&Commands`; o `match` desestrutura cada variante e
        // extrai a referência ao `*Args` interno (`args`), sem mover nada
        // para fora de `self` (por isso funciona com `&self` e não `self`).
        match self {
            Commands::Version(args) => args.execute(),
            Commands::Logs(args) => args.execute(),
            Commands::Ai(args) => args.execute(),
        }
    }
}

/// dev-cli: canivete suíço para tarefas de desenvolvimento.
// Esta é a struct "raiz" do parser: é ela que recebe `#[derive(Parser)]`
// (as demais recebem `Args` ou `Subcommand`) e é ela que `main.rs` chama
// via `Cli::parse()`. O `///` acima vira a descrição geral (`{about}`) que
// aparece no `--help` de topo.
// `#[command(help_template = ...)]`: troca o texto de ajuda padrão do clap
// pelo template compartilhado em `crate::help` (ver `src/help.rs`), para que
// `dev-cli --help` também saia em português e no mesmo formato dos demais
// subcomandos.
// docs: https://docs.rs/clap/latest/clap/trait.Parser.html
// docs: https://docs.rs/clap/latest/clap/_derive/index.html
#[derive(Parser, Debug)]
#[command(help_template = crate::help::SUBCOMANDOS)]
pub struct Cli {
    // O subcomando escolhido pelo usuário (`version` ou `logs ...`).
    // `#[command(subcommand)]`: diz ao clap que este campo não é uma flag
    // comum, e sim "o restante da linha de comando decide qual variante do
    // enum `Commands` preencher aqui".
    // docs: https://docs.rs/clap/latest/clap/_derive/index.html
    #[command(subcommand)]
    pub command: Commands,
}
