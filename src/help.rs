// Modelos de `--help` em português (evita repetir a mesma string literal
// nos 7 lugares onde o `help_template` foi customizado).
pub const SUBCOMANDOS: &str = "{about}\nUso: {usage}\n\nComandos:\n{subcommands}";
pub const ARGUMENTOS: &str = "{about}\nUso: {usage}\n\n{all-args}";

// Cabeçalho compartilhado entre os structs com --options.
pub const OPCOES: &str = "Opções";
pub const ARGUMENTOS_HEADING: &str = "Argumentos";

// Template para comandos que têm subcomando opcional + argumentos
// próprios (ex: `ai stats`, que sem subcomando mostra um dashboard
// combinado, e com um provedor explícito encaminha só para ele).
pub const ARGUMENTOS_SUBCOMANDOS: &str =
    "{about}\nUso: {usage}\n\nArgumentos:\n{positionals}\n\nOpções:\n{options}\n\nComandos:\n{subcommands}";
