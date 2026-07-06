// CASCA DE IO: subcomando `logs remote`.
// Executa `docker logs` via SSH em hosts remotos, com coleta incremental
// opcional via SQLite, modo --watch e TUI interativa.

use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Args;
use owo_colors::OwoColorize;
use rusqlite::Connection;

use crate::logs::core::{categorizar_por_nivel, contar_niveis_docker};
use crate::logs::db::{
    armazenar_contagens, armazenar_linhas, exibir_estatisticas, init_db,
    verificar_status_containers,
};
use crate::logs::render::renderizar_container;

/// Metadados de um container obtidos via `docker ps`.
struct ContainerRemoto {
    nome: String,
    status: String,
    criado_em: String,
}

#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct RemoteArgs {
    /// Container específico; se omitido, varre todos os containers rodando.
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    container: Option<String>,
    /// Host SSH (user@host).
    #[arg(long, default_value = "jarede.silva@qa.bistek.com.br")]
    host: String,
    /// Quantidade de linhas do final de cada container (últimas N linhas).
    #[arg(long, default_value_t = 1000)]
    tail: usize,
    /// Caminho do banco SQLite para armazenamento incremental.
    #[arg(long)]
    db: Option<PathBuf>,
    /// Modo contínuo: coleta a cada 5 minutos (requer --db).
    #[arg(short, long)]
    watch: bool,
    /// Abre TUI interativo para navegar nas estatísticas (requer --db).
    #[arg(long)]
    tui: bool,
}

impl RemoteArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        let Some(db_path) = &self.db else {
            // Modo original: one-shot sem persistência
            let containers = if let Some(container) = &self.container {
                vec![ContainerRemoto {
                    nome: container.clone(),
                    status: String::new(),
                    criado_em: String::new(),
                }]
            } else {
                listar_containers_remoto(&self.host)?
            };
            let mut saida = String::new();
            for c in containers {
                let conteudo = obter_logs_remoto(&self.host, &c.nome, self.tail)?;
                let niveis = contar_niveis_docker(&conteudo);
                let status = if c.status.is_empty() {
                    None
                } else {
                    Some(c.status.as_str())
                };
                saida.push_str(&renderizar_container(&c.nome, status, &niveis));
            }
            return Ok(saida.trim_end().to_string());
        };

        // Modo com banco: coleta incremental + persistência
        let conn = Connection::open(db_path)?;
        init_db(&conn)?;

        let db_vazio = conn
            .query_row("SELECT COUNT(*) FROM containers", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0)
            == 0;

        if db_vazio || !self.tui {
            let agora = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let mut saida = String::new();

            // 1. Descobre containers rodando e detecta paradas/restart
            let rodando = listar_containers_remoto(&self.host)?;
            let nomes_rodando: Vec<String> = rodando.iter().map(|c| c.nome.clone()).collect();
            let alertas = verificar_status_containers(&conn, &nomes_rodando, agora)?;
            for alerta in &alertas {
                saida.push_str(&format!("⚠️  {}\n", alerta.bold()));
            }

            // 2. Coleta incremental dos que estão rodando
            for c in &rodando {
                let ultima_coleta: i64 = conn
                    .query_row(
                        "SELECT COALESCE(last_collected_at, 0) FROM containers WHERE name = ?1",
                        rusqlite::params![c.nome],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                let conteudo = if ultima_coleta == 0 {
                    obter_logs_remoto(&self.host, &c.nome, self.tail)?
                } else {
                    obter_logs_remoto_desde(&self.host, &c.nome, ultima_coleta)?
                };

                let grupos = categorizar_por_nivel(&conteudo);
                let niveis: std::collections::BTreeMap<String, usize> =
                    grupos.iter().map(|(k, v)| (k.clone(), v.len())).collect();

                armazenar_contagens(&conn, &c.nome, &niveis, agora)?;
                armazenar_linhas(&conn, &c.nome, &grupos, agora)?;
                conn.execute(
                    "INSERT OR REPLACE INTO containers (name, status, last_collected_at, uptime, criado_em) VALUES (?1, 'running', ?2, ?3, ?4)",
                    rusqlite::params![c.nome, agora, c.status, c.criado_em],
                )?;
            }

            // 3. Exibe acumulado do banco (só se não for TUI)
            if !self.tui {
                saida.push_str(&exibir_estatisticas(&conn)?);

                if !self.watch {
                    return Ok(saida.trim_end().to_string());
                }

                print!("\x1b[2J\x1b[H{}", saida.trim_end());
                std::io::stdout().flush()?;
                std::thread::sleep(Duration::from_secs(300));
            }
        }

        crate::tui::run_tui(&conn)?;
        Ok(String::new())
    }
}

/// CASCA DE IO: busca logs de um container desde um timestamp Unix (segundos).
fn obter_logs_remoto_desde(
    host: &str,
    nome: &str,
    desde: i64,
) -> Result<String, Box<dyn std::error::Error>> {
    let cmd = format!("docker logs --since {desde} {nome}");
    let saida = std::process::Command::new("ssh")
        .args([host, &cmd])
        .output()
        .map_err(|erro| format!("falha ao obter logs incrementais de '{nome}' via SSH: {erro}"))?;

    if !saida.status.success() {
        return Err(format!(
            "'docker logs --since {desde} {nome}' via SSH terminou com erro: {}",
            String::from_utf8_lossy(&saida.stderr)
        )
        .into());
    }

    Ok(String::from_utf8_lossy(&saida.stdout).to_string())
}

/// CASCA DE IO: pergunta ao host remoto quais containers estão rodando.
fn listar_containers_remoto(
    host: &str,
) -> Result<Vec<ContainerRemoto>, Box<dyn std::error::Error>> {
    let saida = std::process::Command::new("ssh")
        .args([
            host,
            "docker ps --format '{{.Names}}|{{.Status}}|{{.CreatedAt}}'",
        ])
        .output()
        .map_err(|erro| format!("falha ao conectar via SSH em {host}: {erro}"))?;

    if !saida.status.success() {
        return Err(format!(
            "SSH para {host} terminou com erro: {}",
            String::from_utf8_lossy(&saida.stderr)
        )
        .into());
    }

    Ok(String::from_utf8_lossy(&saida.stdout)
        .lines()
        .map(str::trim)
        .filter(|linha| !linha.is_empty())
        .filter_map(|linha| {
            let mut partes = linha.splitn(3, '|');
            Some(ContainerRemoto {
                nome: partes.next()?.to_string(),
                status: partes.next()?.to_string(),
                criado_em: partes.next()?.to_string(),
            })
        })
        .collect())
}

/// CASCA DE IO: busca as últimas N linhas do log de um container via SSH.
fn obter_logs_remoto(
    host: &str,
    nome: &str,
    tail: usize,
) -> Result<String, Box<dyn std::error::Error>> {
    let cmd = if tail > 0 {
        format!("docker logs --tail {tail} {nome}")
    } else {
        format!("docker logs {nome}")
    };
    let saida = std::process::Command::new("ssh")
        .args([host, &cmd])
        .output()
        .map_err(|erro| format!("falha ao obter logs de '{nome}' via SSH: {erro}"))?;

    if !saida.status.success() {
        return Err(format!(
            "'docker logs {nome}' via SSH terminou com erro: {}",
            String::from_utf8_lossy(&saida.stderr)
        )
        .into());
    }

    Ok(String::from_utf8_lossy(&saida.stdout).to_string())
}
