// Modelos de `--help` em português (evita repetir a mesma string literal
// nos 7 lugares onde o `help_template` foi customizado).
pub const SUBCOMANDOS: &str = "{about}\nUso: {usage}\n\nComandos:\n{subcommands}";
pub const ARGUMENTOS: &str = "{about}\nUso: {usage}\n\n{all-args}";

// Cabeçalho compartilhado entre os structs com --options.
pub const OPCOES: &str = "Opções";
pub const ARGUMENTOS_HEADING: &str = "Argumentos";
