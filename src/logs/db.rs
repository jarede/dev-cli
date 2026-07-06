// OPERAÇÕES DE BANCO: inicialização, armazenamento de contagens/linhas,
// verificação de status de containers e exibição do acumulado.

use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::logs::render::renderizar_container;

/// Cria as tabelas do banco se não existirem e executa migrações.
pub(crate) fn init_db(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS containers (
            name TEXT PRIMARY KEY,
            status TEXT NOT NULL DEFAULT 'unknown',
            last_collected_at INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS log_counts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            container_name TEXT NOT NULL,
            level TEXT NOT NULL,
            count INTEGER NOT NULL DEFAULT 0,
            collected_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS alerts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            container_name TEXT NOT NULL,
            alert_type TEXT NOT NULL,
            message TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS log_lines (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            container_name TEXT NOT NULL,
            level TEXT NOT NULL,
            line TEXT NOT NULL,
            collected_at INTEGER NOT NULL
        );",
    )?;

    for sql in &[
        "ALTER TABLE containers ADD COLUMN uptime TEXT DEFAULT ''",
        "ALTER TABLE containers ADD COLUMN criado_em TEXT DEFAULT ''",
    ] {
        let _ = conn.execute(sql, []);
    }

    Ok(())
}

/// Insere as contagens desta coleta no banco.
pub(crate) fn armazenar_contagens(
    conn: &Connection,
    nome: &str,
    niveis: &BTreeMap<String, usize>,
    agora: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stmt =
        conn.prepare("INSERT INTO log_counts (container_name, level, count, collected_at) VALUES (?1, ?2, ?3, ?4)")?;
    for (nivel, &quantidade) in niveis {
        if quantidade > 0 {
            stmt.execute(rusqlite::params![nome, nivel, quantidade as i64, agora])?;
        }
    }
    Ok(())
}

/// Armazena as linhas de log no banco, agrupadas por nível.
/// Remove linhas antigas do mesmo container para evitar acúmulo infinito.
pub(crate) fn armazenar_linhas(
    conn: &Connection,
    nome: &str,
    grupos: &BTreeMap<String, Vec<String>>,
    agora: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "DELETE FROM log_lines WHERE container_name = ?1",
        rusqlite::params![nome],
    )?;

    let mut stmt = conn.prepare(
        "INSERT INTO log_lines (container_name, level, line, collected_at) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for (nivel, linhas) in grupos {
        for linha in linhas {
            stmt.execute(rusqlite::params![nome, nivel, linha, agora])?;
        }
    }
    Ok(())
}

/// Compara containers conhecidos no DB com os que estão rodando agora.
/// Gera alertas para containers que pararam ou reiniciaram.
pub(crate) fn verificar_status_containers(
    conn: &Connection,
    rodando: &[String],
    agora: i64,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut alertas = Vec::new();

    let mut stmt = conn.prepare("SELECT name FROM containers WHERE status = 'running'")?;
    let conhecidos: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for nome in &conhecidos {
        if !rodando.contains(nome) {
            conn.execute(
                "UPDATE containers SET status = 'stopped' WHERE name = ?1",
                rusqlite::params![nome],
            )?;
            conn.execute(
                "INSERT INTO alerts (container_name, alert_type, message, created_at) VALUES (?1, 'stopped', ?2, ?3)",
                rusqlite::params![nome, format!("Container '{nome}' parou"), agora],
            )?;
            alertas.push(format!("⚠️  {} PAROU", nome));
        }
    }

    for nome in rodando {
        let status_anterior: Option<String> = conn
            .query_row(
                "SELECT status FROM containers WHERE name = ?1",
                rusqlite::params![nome],
                |row| row.get(0),
            )
            .ok();

        if let Some(status) = status_anterior.as_ref()
            && status == "stopped"
        {
            conn.execute(
                "INSERT INTO alerts (container_name, alert_type, message, created_at) VALUES (?1, 'restarted', ?2, ?3)",
                rusqlite::params![nome, format!("Container '{nome}' reiniciou"), agora],
            )?;
            alertas.push(format!("🔄 {} REINICIOU", nome));
        }
    }

    Ok(alertas)
}

/// Lê as contagens acumuladas do banco e formata para exibição.
pub(crate) fn exibir_estatisticas(conn: &Connection) -> Result<String, Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT container_name, level, SUM(count) as total
         FROM log_counts
         GROUP BY container_name, level
         ORDER BY container_name, level",
    )?;

    let mut dados: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let linhas = stmt.query_map([], |row| {
        let nome: String = row.get(0)?;
        let nivel: String = row.get(1)?;
        let total: i64 = row.get(2)?;
        Ok((nome, nivel, total as usize))
    })?;

    for linha in linhas {
        let (nome, nivel, total) = linha?;
        dados.entry(nome).or_default().insert(nivel, total);
    }

    let mut stmt2 = conn
        .prepare("SELECT name, uptime FROM containers WHERE uptime IS NOT NULL AND uptime != ''")?;
    let mut status_map: BTreeMap<String, String> = BTreeMap::new();
    for row in stmt2
        .query_map([], |r| {
            let n: String = r.get(0)?;
            let s: String = r.get(1)?;
            Ok((n, s))
        })?
        .flatten()
    {
        status_map.insert(row.0, row.1);
    }

    let mut saida = String::new();
    for (nome, niveis) in &dados {
        let status = status_map.get(nome).map(|s| s.as_str());
        saida.push_str(&renderizar_container(nome, status, niveis));
    }
    Ok(saida)
}
