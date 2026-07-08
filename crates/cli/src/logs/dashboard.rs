// CASCA DE IO: subcomando `logs dashboard` — o modo "ao vivo" do dev-cli.
// Resolve a configuração (flags > env > arquivo > defaults), sobe a thread
// coletora do nucleo e entrega o terminal ao dashboard.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use clap::Args;
use rusqlite::Connection;

use nucleo::coletor::{iniciar_coletor, ComandoColetor, ParametrosColetor};
use nucleo::config::Config;
use nucleo::db::init_db;
use nucleo::executor::Executor;

use crate::screens::dashboard::DashboardScreen;

/// Dashboard ao vivo: coleta contínua + visão de problemas por container.
#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct DashboardArgs {
    /// Caminho do arquivo de configuração TOML.
    /// (default: ~/.config/dev-cli/config.toml, se existir)
    #[arg(long)]
    config: Option<PathBuf>,
    /// Host SSH ("user@host") para coletar de uma VM remota.
    /// Sem esta flag, executa `docker` localmente (modo padrão na VM).
    #[arg(long)]
    ssh: Option<String>,
    /// Caminho do banco SQLite (sobrepõe config/env).
    #[arg(long)]
    db: Option<PathBuf>,
    /// Segundos entre coletas (sobrepõe config/env).
    #[arg(long)]
    intervalo: Option<u64>,
}

impl DashboardArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // 1. Config com precedência: flags (aqui) > env > arquivo > defaults.
        // `as_deref()`: converte `&Option<PathBuf>` em `Option<&Path>` sem
        // clonar — o idioma para passar "talvez um caminho" por referência.
        // docs: https://doc.rust-lang.org/std/option/enum.Option.html#method.as_deref
        let mut config = Config::carregar(self.config.as_deref())?;
        if let Some(ssh) = &self.ssh {
            config.coleta.ssh = ssh.clone();
        }
        if let Some(db) = &self.db {
            config.coleta.db = db.display().to_string();
        }
        if let Some(intervalo) = self.intervalo {
            config.coleta.intervalo_seg = intervalo;
        }

        let executor = if config.coleta.ssh.is_empty() {
            Executor::Local
        } else {
            Executor::Ssh(config.coleta.ssh.clone())
        };
        let origem = if config.coleta.ssh.is_empty() {
            "local".to_string()
        } else {
            format!("ssh: {}", config.coleta.ssh)
        };

        // 2. Banco: garante o diretório e o schema ANTES de subir a thread
        // (as duas conexões — TUI e coletor — apontam para o mesmo arquivo).
        let caminho_db = config.caminho_db();
        if let Some(pai) = caminho_db.parent() {
            std::fs::create_dir_all(pai)?;
        }
        let conn = Connection::open(&caminho_db)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        init_db(&conn)?;

        // 3. Sobe o coletor e abre a TUI com o canal de eventos.
        let (tx_eventos, rx_eventos) = mpsc::channel();
        let (handle, tx_comandos) = iniciar_coletor(
            ParametrosColetor {
                executor,
                db: caminho_db,
                intervalo: Duration::from_secs(config.coleta.intervalo_seg),
                tail_inicial: config.coleta.tail_inicial,
                retencao_horas: config.coleta.retencao_horas,
            },
            tx_eventos,
        );

        let tela = DashboardScreen::new(
            &conn,
            config.limiares.clone(),
            config.coleta.janela_min,
            origem,
            tx_comandos.clone(),
        )?;
        let resultado = crate::tui::run_tui(&conn, Box::new(tela), Some(rx_eventos));

        // 4. Encerramento limpo: pede para a thread parar e espera.
        // (Se um ciclo estiver no meio de um `docker logs`, o join espera
        // ele terminar — aceitável para um comando interativo.)
        let _ = tx_comandos.send(ComandoColetor::Encerrar);
        let _ = handle.join();

        resultado?;
        Ok(String::new())
    }
}
