// Configuração do dev-cli. O projeto é público/open source: NADA de host,
// caminho ou credencial hardcoded no código — tudo vem daqui.
//
// Precedência (do mais forte para o mais fraco):
//   flags da CLI  >  variáveis de ambiente DEV_CLI_*  >  arquivo TOML  >  defaults
//
// A parte PURA (parse do TOML, aplicação de env vars sobre a struct) é
// testável com strings inline; a CASCA (`carregar`) lê arquivo/ambiente.

use std::path::{Path, PathBuf};

// `Deserialize`: derive do serde que gera o código de conversão
// TOML -> struct em tempo de compilação.
// docs: https://docs.rs/serde/latest/serde/trait.Deserialize.html
use serde::Deserialize;

/// Configuração completa, espelhando o arquivo TOML:
///
/// ```toml
/// [coleta]
/// intervalo_seg = 60
/// janela_min = 15
/// retencao_horas = 24
/// tail_inicial = 1000
/// db = "~/.local/share/dev-cli/logs.db"
/// ssh = ""            # vazio = docker local; "user@host" = via SSH
///
/// [limiares]
/// p95_lento_seg = 1.0
/// taxa_erro_pct = 5.0
///
/// [servidor]
/// bind = "127.0.0.1:8787"
/// ```
// `#[serde(default)]`: se uma seção/campo faltar no TOML, usa o `Default`
// correspondente em vez de falhar — permite arquivos parciais.
// docs: https://serde.rs/container-attrs.html#default
#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub coleta: Coleta,
    pub limiares: Limiares,
    pub servidor: Servidor,
}

/// Parâmetros da coleta de logs.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Coleta {
    /// Segundos entre um ciclo de coleta e o próximo.
    pub intervalo_seg: u64,
    /// Janela (minutos) considerada nas estatísticas do dashboard.
    pub janela_min: u64,
    /// Depois de quantas horas linhas/requests antigas são apagadas do banco.
    pub retencao_horas: u64,
    /// Linhas do `docker logs --tail` na PRIMEIRA coleta de um container
    /// (as seguintes são incrementais com `--since`).
    pub tail_inicial: usize,
    /// Caminho do SQLite. Vazio = default do sistema (~/.local/share/dev-cli/logs.db).
    pub db: String,
    /// Host SSH ("user@host"). Vazio = docker local (modo padrão na VM).
    pub ssh: String,
}

// `Default` implementado à mão (em vez de `#[derive(Default)]`) porque os
// defaults do projeto NÃO são os "zero values" do Rust (0, "").
// docs: https://doc.rust-lang.org/std/default/trait.Default.html
impl Default for Coleta {
    fn default() -> Self {
        Self {
            intervalo_seg: 60,
            janela_min: 15,
            retencao_horas: 24,
            tail_inicial: 1000,
            db: String::new(),
            ssh: String::new(),
        }
    }
}

/// Limiares que separam verde/amarelo/vermelho no dashboard.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Limiares {
    /// p95 de tempo de resposta (segundos) acima do qual o container fica amarelo.
    pub p95_lento_seg: f64,
    /// Percentual de linhas ERROR/CRIT acima do qual o container fica vermelho.
    pub taxa_erro_pct: f64,
}

impl Default for Limiares {
    fn default() -> Self {
        Self {
            p95_lento_seg: 1.0,
            taxa_erro_pct: 5.0,
        }
    }
}

/// Configuração do servidor HTTP (`crates/servidor`, Fase 2).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Servidor {
    /// Endereço de escuta da API ("host:porta"). Localhost por padrão:
    /// expor para a rede deve ser uma decisão EXPLÍCITA do operador
    /// (mudar para "0.0.0.0:8787" na config ou usar um proxy reverso).
    pub bind: String,
}

// Mesmo motivo do `Default` manual de `Coleta`: o default do projeto não é
// o "zero value" (String vazia) do derive.
// docs: https://doc.rust-lang.org/std/default/trait.Default.html
impl Default for Servidor {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1:8787".to_string(),
        }
    }
}

impl Config {
    /// NÚCLEO PURO: parseia um TOML (possivelmente parcial) numa Config.
    pub fn de_toml(texto: &str) -> Result<Config, toml::de::Error> {
        // `toml::from_str` usa o `Deserialize` derivado acima.
        // docs: https://docs.rs/toml/latest/toml/fn.from_str.html
        toml::from_str(texto)
    }

    /// NÚCLEO PURO: aplica variáveis de ambiente `DEV_CLI_*` sobre a config.
    /// Recebe as vars como iterador de pares (nome, valor) — em produção vem
    /// de `std::env::vars()`, nos testes de um `Vec` inline.
    /// Valores que não parseiam (ex.: "abc" para um número) são ignorados
    /// silenciosamente: preferimos rodar com o default a abortar por causa
    /// de uma env var malformada de outro processo.
    pub fn aplicar_env<I>(&mut self, vars: I)
    where
        // `IntoIterator`: aceita Vec, array, iterador de env::vars()...
        // docs: https://doc.rust-lang.org/std/iter/trait.IntoIterator.html
        I: IntoIterator<Item = (String, String)>,
    {
        for (nome, valor) in vars {
            // `match` + `if let Ok(...)` = só sobrescreve quando o parse dá certo.
            match nome.as_str() {
                "DEV_CLI_COLETA_INTERVALO_SEG" => {
                    if let Ok(n) = valor.parse() {
                        self.coleta.intervalo_seg = n;
                    }
                }
                "DEV_CLI_COLETA_JANELA_MIN" => {
                    if let Ok(n) = valor.parse() {
                        self.coleta.janela_min = n;
                    }
                }
                "DEV_CLI_COLETA_RETENCAO_HORAS" => {
                    if let Ok(n) = valor.parse() {
                        self.coleta.retencao_horas = n;
                    }
                }
                "DEV_CLI_COLETA_TAIL_INICIAL" => {
                    if let Ok(n) = valor.parse() {
                        self.coleta.tail_inicial = n;
                    }
                }
                "DEV_CLI_COLETA_DB" => self.coleta.db = valor,
                "DEV_CLI_COLETA_SSH" => self.coleta.ssh = valor,
                "DEV_CLI_LIMIARES_P95_LENTO_SEG" => {
                    if let Ok(n) = valor.parse() {
                        self.limiares.p95_lento_seg = n;
                    }
                }
                "DEV_CLI_LIMIARES_TAXA_ERRO_PCT" => {
                    if let Ok(n) = valor.parse() {
                        self.limiares.taxa_erro_pct = n;
                    }
                }
                "DEV_CLI_SERVIDOR_BIND" => self.servidor.bind = valor,
                _ => {}
            }
        }
    }

    /// CASCA DE IO: resolve a config completa respeitando a precedência
    /// arquivo < env. (As flags da CLI são aplicadas pelo chamador, que é
    /// quem as conhece.) Arquivo ausente NÃO é erro (usa defaults); arquivo
    /// presente mas inválido É erro (avisa o usuário em vez de ignorar).
    pub fn carregar(caminho: Option<&Path>) -> Result<Config, Box<dyn std::error::Error>> {
        // Carrega um `.env` do diretório atual, se existir (útil em dev).
        // O resultado é ignorado de propósito: sem `.env` não é erro.
        // docs: https://docs.rs/dotenvy/latest/dotenvy/fn.dotenv.html
        let _ = dotenvy::dotenv();

        // `--config` explícito > caminho padrão do sistema.
        // `dirs::config_dir()`: ~/.config no Linux, ~/Library/Application Support no macOS.
        // docs: https://docs.rs/dirs/latest/dirs/fn.config_dir.html
        let caminho_arquivo: Option<PathBuf> = match caminho {
            Some(c) => Some(c.to_path_buf()),
            None => dirs::config_dir().map(|d| d.join("dev-cli").join("config.toml")),
        };

        let mut config = match caminho_arquivo {
            Some(c) if c.exists() => {
                let texto = std::fs::read_to_string(&c)?;
                Config::de_toml(&texto)
                    .map_err(|erro| format!("config inválida em {}: {erro}", c.display()))?
            }
            // Sem arquivo: começa dos defaults.
            _ => Config::default(),
        };

        config.aplicar_env(std::env::vars());
        Ok(config)
    }

    /// Resolve o caminho final do banco: campo `db` (com `~` expandido) ou
    /// o diretório de dados do sistema (~/.local/share/dev-cli/logs.db).
    pub fn caminho_db(&self) -> PathBuf {
        if self.coleta.db.is_empty() {
            return dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("dev-cli")
                .join("logs.db");
        }
        expandir_til(&self.coleta.db)
    }
}

/// Expande um `~/` inicial para o diretório home do usuário.
/// (O shell faz isso em argumentos de linha de comando, mas NÃO em valores
/// vindos de arquivo de config — por isso fazemos manualmente.)
fn expandir_til(caminho: &str) -> PathBuf {
    if let Some(resto) = caminho.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(resto);
    }
    PathBuf::from(caminho)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_sao_os_documentados() {
        let c = Config::default();
        assert_eq!(c.coleta.intervalo_seg, 60);
        assert_eq!(c.coleta.janela_min, 15);
        assert_eq!(c.coleta.retencao_horas, 24);
        assert_eq!(c.coleta.tail_inicial, 1000);
        assert_eq!(c.coleta.db, "");
        assert_eq!(c.coleta.ssh, "");
        assert_eq!(c.limiares.p95_lento_seg, 1.0);
        assert_eq!(c.limiares.taxa_erro_pct, 5.0);
    }

    #[test]
    fn toml_parcial_preserva_defaults_do_resto() {
        let c = Config::de_toml("[coleta]\nintervalo_seg = 30\n").unwrap();
        assert_eq!(c.coleta.intervalo_seg, 30);
        // O resto continua nos defaults.
        assert_eq!(c.coleta.janela_min, 15);
        assert_eq!(c.limiares.taxa_erro_pct, 5.0);
    }

    #[test]
    fn toml_completo() {
        let texto = r#"
[coleta]
intervalo_seg = 10
janela_min = 5
retencao_horas = 48
tail_inicial = 200
db = "/tmp/x.db"
ssh = "eu@host"

[limiares]
p95_lento_seg = 2.5
taxa_erro_pct = 1.0
"#;
        let c = Config::de_toml(texto).unwrap();
        assert_eq!(c.coleta.ssh, "eu@host");
        assert_eq!(c.coleta.db, "/tmp/x.db");
        assert_eq!(c.limiares.p95_lento_seg, 2.5);
    }

    #[test]
    fn toml_invalido_e_erro() {
        assert!(Config::de_toml("isto nao é toml [").is_err());
    }

    #[test]
    fn env_sobrepoe_arquivo() {
        let mut c = Config::de_toml("[coleta]\nintervalo_seg = 30\n").unwrap();
        c.aplicar_env(vec![
            ("DEV_CLI_COLETA_INTERVALO_SEG".to_string(), "10".to_string()),
            ("DEV_CLI_COLETA_SSH".to_string(), "dev@qa".to_string()),
            ("DEV_CLI_LIMIARES_TAXA_ERRO_PCT".to_string(), "2.5".to_string()),
        ]);
        assert_eq!(c.coleta.intervalo_seg, 10);
        assert_eq!(c.coleta.ssh, "dev@qa");
        assert_eq!(c.limiares.taxa_erro_pct, 2.5);
    }

    #[test]
    fn env_com_valor_invalido_e_ignorada() {
        let mut c = Config::default();
        c.aplicar_env(vec![(
            "DEV_CLI_COLETA_INTERVALO_SEG".to_string(),
            "abc".to_string(),
        )]);
        assert_eq!(c.coleta.intervalo_seg, 60); // manteve o default
    }

    #[test]
    fn env_desconhecida_e_ignorada() {
        let mut c = Config::default();
        c.aplicar_env(vec![("PATH".to_string(), "/usr/bin".to_string())]);
        assert_eq!(c, Config::default());
    }

    #[test]
    fn caminho_db_expande_til() {
        let mut c = Config::default();
        c.coleta.db = "~/x/logs.db".to_string();
        let caminho = c.caminho_db();
        assert!(caminho.ends_with("x/logs.db"));
        assert!(!caminho.to_string_lossy().contains('~'));
    }

    #[test]
    fn servidor_default_e_localhost() {
        assert_eq!(Config::default().servidor.bind, "127.0.0.1:8787");
    }

    #[test]
    fn toml_configura_bind_do_servidor() {
        let c = Config::de_toml("[servidor]\nbind = \"0.0.0.0:9000\"\n").unwrap();
        assert_eq!(c.servidor.bind, "0.0.0.0:9000");
        // O resto continua nos defaults.
        assert_eq!(c.coleta.intervalo_seg, 60);
    }

    #[test]
    fn env_sobrepoe_bind_do_servidor() {
        let mut c = Config::default();
        c.aplicar_env(vec![(
            "DEV_CLI_SERVIDOR_BIND".to_string(),
            "0.0.0.0:1234".to_string(),
        )]);
        assert_eq!(c.servidor.bind, "0.0.0.0:1234");
    }
}
