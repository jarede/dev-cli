// Modelos de `--help` em português (evita repetir a mesma string literal
// nos 7 lugares onde o `help_template` foi customizado).
//
// O clap, por padrão, gera o texto de `--help` em inglês e com um layout
// próprio. `#[command(help_template = "...")]` permite substituir esse
// layout por um template textual com "placeholders" entre chaves, que o
// clap substitui em tempo de execução pelo conteúdo correspondente:
//   {about}       -> a doc string (`///`) da struct/enum anotada.
//   {usage}       -> a linha "Usage: dev-cli <comando> [opções]" montada
//                    automaticamente a partir dos argumentos declarados.
//   {subcommands} -> lista dos subcomandos disponíveis (nome + `///` de cada).
//   {all-args}    -> lista combinada de todos os argumentos/flags.
//   {positionals} -> só os argumentos posicionais (sem `--flag`).
//   {options}     -> só as flags/opções (com `--flag`).
// Todas as consts abaixo são `&str` (fatias de string), e como são strings
// literais (`"..."`), elas têm lifetime `'static`: vivem por todo o programa,
// então não precisam de `String` alocada nem de gerenciamento de memória.
// Cada `#[command(help_template = crate::help::X)]` no restante do projeto
// aponta para uma destas constantes, evitando repetir o mesmo texto de
// template (e o mesmo erro de digitação) em cada arquivo.
pub const SUBCOMANDOS: &str = "{about}\nUso: {usage}\n\nComandos:\n{subcommands}";
pub const ARGUMENTOS: &str = "{about}\nUso: {usage}\n\n{all-args}";

// Cabeçalho compartilhado entre os structs com --options. Usadas com
// `#[arg(help_heading = ...)]`/`#[command(next_help_heading = ...)]` para
// agrupar, no `--help`, os argumentos posicionais sob "Argumentos" e as
// flags sob "Opções" (em vez do "Arguments"/"Options" padrão do clap).
pub const OPCOES: &str = "Opções";
pub const ARGUMENTOS_HEADING: &str = "Argumentos";

// Template para comandos que têm subcomando opcional + argumentos
// próprios (ex: `ai stats`, que sem subcomando mostra um dashboard
// combinado, e com um provedor explícito encaminha só para ele). Diferente
// de `ARGUMENTOS` (que usa o bloco pronto `{all-args}`), este template
// separa manualmente `{positionals}`, `{options}` e `{subcommands}` em
// seções próprias, porque aqui coexistem os três tipos ao mesmo tempo.
pub const ARGUMENTOS_SUBCOMANDOS: &str =
    "{about}\nUso: {usage}\n\nArgumentos:\n{positionals}\n\nOpções:\n{options}\n\nComandos:\n{subcommands}";
