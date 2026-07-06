// Declaração dos módulos do binário. Cada `mod` corresponde a um arquivo (ou
// pasta com `mod.rs`/`nome.rs`) em src/. Sem essas linhas, o compilador nem
// enxergaria os arquivos `ai.rs`/`ai/mod.rs`, `cli.rs`, `help.rs`, `logs.rs`
// e `tui.rs` — em Rust, diferente de outras linguagens, a árvore de módulos
// é declarada explicitamente a partir da raiz (`main.rs`), e não inferida
// pela estrutura de pastas sozinha.
mod ai;
mod cli;
mod help;
mod logs;
mod tui;

// Traz o método `parse()` (da trait `Parser`) para o escopo. Só declarar
// `use clap::Parser` já é suficiente: não chamamos nada da trait
// diretamente aqui, mas o `derive(Parser)` em `cli.rs` gera uma
// implementação da trait para `Cli`, e `Cli::parse()` (chamado abaixo) só
// está disponível para nós porque a trait está "à vista" (in scope).
use clap::Parser;
use cli::Cli;

// `main` é o ponto de entrada de qualquer binário Rust. Note que não devolve
// `Result` (diferente das funções `execute()` do resto do projeto): erros
// daqui em diante são tratados "na mão", terminando o processo explicitamente
// com `std::process::exit`, em vez de propagados com `?` para um chamador.
fn main() {
    // Lê os argumentos da linha de comando (a partir de `std::env::args`) e
    // preenche a struct `Cli` conforme as anotações do clap; em erro de
    // parsing ou quando o usuário pede `--help`/`--version`, o clap já
    // imprime a mensagem apropriada e encerra o processo por conta própria
    // (chamando `exit` internamente) — por isso `Cli::parse()` não devolve
    // `Result`: se chegou a devolver algo, é porque deu certo.
    let cli = Cli::parse();
    // Executa o subcomando escolhido e trata o `Result` com `match`: como
    // `Result<T, E>` é um enum com duas variantes (`Ok`/`Err`), o `match`
    // exaustivo obriga a tratar as duas — não dá para "esquecer" o caminho
    // de erro como aconteceria com uma exceção não capturada em outras
    // linguagens.
    match cli.command.execute() {
        // Sucesso: imprime o texto em stdout. `output` já é a `String`
        // pronta que cada `execute()` monta; aqui só formatamos e mostramos.
        Ok(output) => println!("{output}"),
        // Falha: mensagem em stderr e código de saída 1 (convenção Unix de
        // processo que terminou com erro). `{error}` usa o `Display` do
        // erro (mensagem amigável para o usuário); trocar para `{error:?}`
        // (Debug) mostraria mais detalhes internos, útil ao depurar, mas
        // menos legível para quem só quer saber "o que deu errado".
        Err(error) => {
            eprintln!("{error}");
            // `std::process::exit` encerra o processo imediatamente com o
            // código passado, sem rodar destructors (`Drop`) pendentes na
            // pilha — diferente de um `return` normal de `main`. Como aqui
            // não há nenhum recurso importante para limpar antes de sair
            // (arquivos, conexões etc. já foram fechados dentro do
            // `execute()` que falhou), isso é seguro.
            std::process::exit(1)
        }
    }
}
