// Declaração dos módulos do binário. Cada `mod` corresponde a um arquivo em src/.
mod ai;
mod cli;
mod help;
mod logs;
mod tui;

// Traz o método `parse()` (da trait `Parser`) para o escopo.
use clap::Parser;
use cli::Cli;

fn main() {
    // Lê os argumentos da linha de comando; em erro/`--help`, o clap já imprime
    // a mensagem e encerra o processo por conta própria.
    let cli = Cli::parse();
    // Executa o subcomando escolhido e trata o `Result`:
    match cli.command.execute() {
        // Sucesso: imprime o texto em stdout.
        Ok(output) => println!("{output}"),
        // Falha: mensagem em stderr e código de saída 1 (convenção Unix de erro).
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1)
        }
    }
}
