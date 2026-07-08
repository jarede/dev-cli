// Grupo de subcomandos `logs dashboard`, `logs stats`, `logs containers` e
// `logs remote`.
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
mod dashboard;
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
    // `logs` sempre encaminha para um subcomando aninhado; sem `Option`, o
    // clap exige que o usuário informe um (`stats`, `containers`, `remote`
    // ou `dashboard`) e cuida da mensagem de erro sozinho quando não informa.
    #[command(subcommand)]
    comando: LogsCommands,
}

impl LogsArgs {
    // `&self` = empréstimo imutável; só lemos os campos, não os consumimos.
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Delega para o subcomando escolhido.
        match &self.comando {
            LogsCommands::Dashboard(args) => args.execute(),
            LogsCommands::Stats(args) => args.execute(),
            LogsCommands::Containers(args) => args.execute(),
            LogsCommands::Remote(args) => args.execute(),
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
    /// Dashboard ao vivo: onde estão os problemas nos containers.
    Dashboard(dashboard::DashboardArgs),
    /// Estatísticas de logs de containers (arquivos supervisord).
    Stats(stats::StatsArgs),
    /// Estatísticas de logs dos containers detectados via `container list`.
    Containers(containers::ContainersArgs),
    /// Estatísticas de logs de containers via SSH (docker logs remoto).
    Remote(remote::RemoteArgs),
}
