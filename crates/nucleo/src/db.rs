// OPERAÇÕES DE BANCO: inicialização, armazenamento de contagens/linhas,
// verificação de status de containers e exibição do acumulado.

use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::core::LoguruEntry;
use crate::metricas::{ResumoContainer, p95};

/// Cria as tabelas do banco se não existirem e executa migrações.
// docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.execute_batch
pub fn init_db(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
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
        );
        CREATE TABLE IF NOT EXISTS requests (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            container_name TEXT NOT NULL,
            ts TEXT NOT NULL,
            metodo TEXT NOT NULL,
            path TEXT NOT NULL,
            status INTEGER NOT NULL,
            duracao_seg REAL NOT NULL,
            tenant TEXT NOT NULL,
            collected_at INTEGER NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_requests_container_ts
            ON requests (container_name, collected_at);
        CREATE INDEX IF NOT EXISTS idx_log_lines_container_ts
            ON log_lines (container_name, collected_at);
        CREATE INDEX IF NOT EXISTS idx_log_counts_container_ts
            ON log_counts (container_name, collected_at);",
    )?;

    // Migração: adiciona colunas que podem não existir em DBs criados antes
    // (o `CREATE TABLE IF NOT EXISTS` acima não altera tabelas já existentes).
    // `&[...]` é um array de literais `&str` percorrido por referência.
    for sql in &[
        "ALTER TABLE containers ADD COLUMN uptime TEXT DEFAULT ''",
        "ALTER TABLE containers ADD COLUMN criado_em TEXT DEFAULT ''",
    ] {
        // `let _ = ...` descarta o `Result` de propósito: se a coluna já
        // existir, o SQLite retorna erro e é exatamente isso que ignoramos
        // aqui (idempotência da migração), sem propagar para o `?` do retorno.
        // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.execute
        let _ = conn.execute(sql, []);
    }

    Ok(())
}

/// Insere as contagens desta coleta no banco.
pub fn armazenar_contagens(
    conn: &Connection,
    nome: &str,
    niveis: &BTreeMap<String, usize>,
    agora: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    // `prepare` compila o SQL uma única vez; `stmt.execute` é chamado depois
    // dentro do loop, reaproveitando a mesma statement preparada (mais
    // eficiente do que preparar um SQL novo a cada nível).
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.prepare
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Statement.html#method.execute
    let mut stmt =
        conn.prepare("INSERT INTO log_counts (container_name, level, count, collected_at) VALUES (?1, ?2, ?3, ?4)")?;
    // `for (nivel, &quantidade) in niveis`: itera pelas entradas do
    // `BTreeMap` desestruturando a tupla `(&String, &usize)`; o padrão
    // `&quantidade` copia o `usize` para fora da referência (tipo `Copy`).
    // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
    // docs: https://doc.rust-lang.org/std/marker/trait.Copy.html
    for (nivel, &quantidade) in niveis {
        if quantidade > 0 {
            // docs: https://docs.rs/rusqlite/latest/rusqlite/macro.params.html
            stmt.execute(rusqlite::params![nome, nivel, quantidade as i64, agora])?;
        }
    }
    Ok(())
}

/// CASCA DE IO: armazena as linhas de log no banco, agrupadas por nível.
/// A retenção não é mais feita aqui — veja `prune_antigos`, que apaga por
/// tempo, permitindo somar a janela através de várias coletas.
pub fn armazenar_linhas(
    conn: &Connection,
    nome: &str,
    grupos: &BTreeMap<String, Vec<String>>,
    agora: i64,
) -> Result<(), Box<dyn std::error::Error>> {
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
pub fn verificar_status_containers(
    conn: &Connection,
    rodando: &[String],
    agora: i64,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut alertas = Vec::new();

    // Containers que estavam running mas não estão mais → pararam
    let mut stmt = conn.prepare("SELECT name FROM containers WHERE status = 'running'")?;
    // `query_map` devolve um iterador de `Result<String, rusqlite::Error>`
    // (uma linha pode falhar ao ser convertida). `filter_map(|r| r.ok())`
    // descarta silenciosamente qualquer linha com erro e mantém só os `Ok`,
    // convertendo cada `Result` em `Option` e já "achatando" o iterador.
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Statement.html#method.query_map
    // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.filter_map
    // docs: https://doc.rust-lang.org/std/result/enum.Result.html#method.ok
    // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.collect
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

    // Containers rodando agora mas estavam stopped → reiniciaram
    for nome in rodando {
        // `.ok()` converte o `Result<String, _>` em `Option<String>`,
        // tratando "não achei essa linha" e "erro de SQL" da mesma forma:
        // simplesmente `None` (sem status anterior conhecido).
        // docs: https://doc.rust-lang.org/std/result/enum.Result.html#method.ok
        let status_anterior: Option<String> = conn
            .query_row(
                "SELECT status FROM containers WHERE name = ?1",
                rusqlite::params![nome],
                |row| row.get(0),
            )
            .ok();

        // Let chain (edition 2024): só entra no bloco se `status_anterior`
        // for `Some` E o valor dentro for exatamente "stopped" — equivalente
        // a um `if aninhado`, mas sem o aninhamento (evita o lint
        // `collapsible_if`). `.as_ref()` empresta o `String` de dentro do
        // `Option` em vez de movê-lo, porque ainda usamos `status_anterior`
        // implicitamente via `status` logo abaixo.
        // docs: https://doc.rust-lang.org/std/option/enum.Option.html#method.as_ref
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

/// Persiste as requests HTTP parseadas (formato Loguru) desta coleta.
pub fn armazenar_requests(
    conn: &Connection,
    nome: &str,
    entradas: &[LoguruEntry],
    agora: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Uma statement preparada reutilizada no loop (mais rápido que preparar
    // SQL novo por linha) — mesmo padrão de `armazenar_contagens`.
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.prepare
    let mut stmt = conn.prepare(
        "INSERT INTO requests (container_name, ts, metodo, path, status, duracao_seg, tenant, collected_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for e in entradas {
        stmt.execute(rusqlite::params![
            nome,
            e.timestamp,
            e.metodo,
            e.path,
            e.status,
            e.duracao_seg,
            e.tenant,
            agora
        ])?;
    }
    Ok(())
}

/// Apaga dados mais antigos que `corte` (timestamp Unix) — a retenção do
/// banco. Chamado a cada ciclo de coleta.
pub fn prune_antigos(conn: &Connection, corte: i64) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute(
        "DELETE FROM log_lines WHERE collected_at < ?1",
        rusqlite::params![corte],
    )?;
    conn.execute(
        "DELETE FROM requests WHERE collected_at < ?1",
        rusqlite::params![corte],
    )?;
    conn.execute(
        "DELETE FROM log_counts WHERE collected_at < ?1",
        rusqlite::params![corte],
    )?;
    conn.execute(
        "DELETE FROM alerts WHERE created_at < ?1",
        rusqlite::params![corte],
    )?;
    Ok(())
}

/// Uma linha de log carregada do banco — o item das telas de drill-down e
/// do endpoint `/api/containers/{nome}/linhas` da Fase 2.
// `serde::Serialize` com caminho completo: evita um `use` só para o derive.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LinhaLog {
    pub nivel: String,
    pub linha: String,
    pub collected_at: i64,
}

/// Converte uma linha do resultado SQL em `LinhaLog`. Extraída como função
/// nomeada (em vez de closure) para ser reutilizada nas DUAS queries de
/// `carregar_linhas_janela` — closures têm tipos anônimos e não podem ser
/// "coladas" em dois `query_map` diferentes com facilidade.
// docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Row.html
fn mapear_linha(row: &rusqlite::Row<'_>) -> rusqlite::Result<LinhaLog> {
    Ok(LinhaLog {
        nivel: row.get(0)?,
        linha: row.get(1)?,
        collected_at: row.get(2)?,
    })
}

/// Linhas de um container dentro da janela (`collected_at >= corte`), das
/// mais recentes para as mais antigas, opcionalmente filtradas por nível.
/// `limite` protege a API de respostas gigantes.
///
/// Dois SQLs fixos em vez de concatenar o filtro na string: SQL montado por
/// concatenação é a porta clássica de SQL injection; com statements fixas e
/// parâmetros `?N` o rusqlite escapa tudo por nós.
// docs: https://docs.rs/rusqlite/latest/rusqlite/macro.params.html
pub fn carregar_linhas_janela(
    conn: &Connection,
    nome: &str,
    nivel: Option<&str>,
    corte: i64,
    limite: usize,
) -> Result<Vec<LinhaLog>, Box<dyn std::error::Error>> {
    let mut resultado = Vec::new();
    if let Some(nivel) = nivel {
        let mut stmt = conn.prepare(
            "SELECT level, line, collected_at FROM log_lines
             WHERE container_name = ?1 AND level = ?2 AND collected_at >= ?3
             ORDER BY collected_at DESC, id DESC LIMIT ?4",
        )?;
        let linhas = stmt.query_map(
            rusqlite::params![nome, nivel, corte, limite as i64],
            mapear_linha,
        )?;
        resultado.extend(linhas.filter_map(|r| r.ok()));
    } else {
        let mut stmt = conn.prepare(
            "SELECT level, line, collected_at FROM log_lines
             WHERE container_name = ?1 AND collected_at >= ?2
             ORDER BY collected_at DESC, id DESC LIMIT ?3",
        )?;
        let linhas = stmt.query_map(rusqlite::params![nome, corte, limite as i64], mapear_linha)?;
        resultado.extend(linhas.filter_map(|r| r.ok()));
    }
    Ok(resultado)
}

/// Um alerta persistido (container parou/reiniciou) — item do endpoint
/// `/api/alertas` da Fase 2.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Alerta {
    pub container: String,
    pub tipo: String,
    pub mensagem: String,
    pub criado_em: i64,
}

/// Alertas com `created_at >= corte`, mais recentes primeiro, até `limite`.
pub fn alertas_recentes(
    conn: &Connection,
    corte: i64,
    limite: usize,
) -> Result<Vec<Alerta>, Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT container_name, alert_type, message, created_at FROM alerts
         WHERE created_at >= ?1 ORDER BY created_at DESC, id DESC LIMIT ?2",
    )?;
    let alertas = stmt.query_map(rusqlite::params![corte, limite as i64], |row| {
        Ok(Alerta {
            container: row.get(0)?,
            tipo: row.get(1)?,
            mensagem: row.get(2)?,
            criado_em: row.get(3)?,
        })
    })?;
    Ok(alertas.filter_map(|r| r.ok()).collect())
}

/// Monta o resumo por container considerando só a janela `collected_at >= corte`.
/// Contagens vêm do SQL (rápido); p95/máx são calculados em Rust a partir das
/// durações da janela (SQLite não tem percentil nativo).
pub fn resumo_janela(
    conn: &Connection,
    corte: i64,
) -> Result<Vec<ResumoContainer>, Box<dyn std::error::Error>> {
    // 1. Base: todos os containers conhecidos, com status e última coleta.
    let mut stmt = conn
        .prepare("SELECT name, status, uptime, last_collected_at FROM containers ORDER BY name")?;
    let mut resumos: Vec<ResumoContainer> = stmt
        .query_map([], |r| {
            Ok(ResumoContainer {
                nome: r.get(0)?,
                status: r.get(1)?,
                uptime: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                ultima_coleta: r.get(3)?,
                ..Default::default()
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    for resumo in &mut resumos {
        // 2. Contagens por nível na janela (a partir de log_counts).
        let mut stmt = conn.prepare(
            "SELECT level, SUM(count) FROM log_counts
             WHERE container_name = ?1 AND collected_at >= ?2 GROUP BY level",
        )?;
        let niveis = stmt.query_map(rusqlite::params![resumo.nome, corte], |r| {
            let nivel: String = r.get(0)?;
            let total: i64 = r.get(1)?;
            Ok((nivel, total))
        })?;
        for par in niveis.filter_map(|r| r.ok()) {
            let (nivel, total) = par;
            resumo.total_linhas += total;
            match nivel.to_uppercase().as_str() {
                "ERROR" | "ERRO" => resumo.erros += total,
                "CRITICAL" | "CRIT" | "FATAL" => resumo.crits += total,
                _ => {}
            }
        }

        // 3. Requests na janela: contagens por classe de status via SQL...
        let (reqs, c5xx, c4xx): (i64, i64, i64) = conn.query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(status BETWEEN 500 AND 599), 0),
                    COALESCE(SUM(status BETWEEN 400 AND 499), 0)
             FROM requests WHERE container_name = ?1 AND collected_at >= ?2",
            rusqlite::params![resumo.nome, corte],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        resumo.reqs = reqs;
        resumo.c5xx = c5xx;
        resumo.c4xx = c4xx;

        // 4. ...e durações trazidas para o Rust para p95/máx.
        let mut stmt = conn.prepare(
            "SELECT duracao_seg FROM requests
             WHERE container_name = ?1 AND collected_at >= ?2",
        )?;
        let duracoes: Vec<f64> = stmt
            .query_map(rusqlite::params![resumo.nome, corte], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        resumo.p95_seg = p95(&duracoes);
        // `fold` com `f64::max` em vez de `.max()` porque f64 não é `Ord`.
        // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.fold
        resumo.max_seg = if duracoes.is_empty() {
            None
        } else {
            Some(duracoes.iter().fold(f64::MIN, |a, &b| a.max(b)))
        };
    }

    Ok(resumos)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parse_loguru_line;

    /// Banco em memória com o schema criado — cada teste parte do zero.
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.open_in_memory
    fn banco() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    fn inserir_container(conn: &Connection, nome: &str, status: &str, agora: i64) {
        conn.execute(
            "INSERT OR REPLACE INTO containers (name, status, last_collected_at, uptime, criado_em)
             VALUES (?1, ?2, ?3, 'Up 1 day', '')",
            rusqlite::params![nome, status, agora],
        )
        .unwrap();
    }

    #[test]
    fn resumo_janela_agrega_contagens_e_requests() {
        let conn = banco();
        inserir_container(&conn, "app", "running", 1000);

        // Contagens: 2 ERROR + 8 INFO dentro da janela, 5 ERROR fora.
        let mut niveis = std::collections::BTreeMap::new();
        niveis.insert("ERROR".to_string(), 2usize);
        niveis.insert("INFO".to_string(), 8usize);
        armazenar_contagens(&conn, "app", &niveis, 1000).unwrap();
        let mut antigos = std::collections::BTreeMap::new();
        antigos.insert("ERROR".to_string(), 5usize);
        armazenar_contagens(&conn, "app", &antigos, 10).unwrap();

        // Uma request 200 e uma 500 dentro da janela (linha Loguru real).
        let linha = "2026-07-07 10:00:00.000 |INFO     | server:http_request:112 - [acme] GET 200 /api/x  0.150s [10.0.0.1] [curl]";
        let e200 = parse_loguru_line(linha).unwrap();
        let mut e500 = e200.clone();
        e500.status = 500;
        e500.duracao_seg = 2.0;
        armazenar_requests(&conn, "app", &[e200, e500], 1000).unwrap();

        let resumos = resumo_janela(&conn, 500).unwrap();
        assert_eq!(resumos.len(), 1);
        let r = &resumos[0];
        assert_eq!(r.nome, "app");
        assert_eq!(r.erros, 2); // os 5 antigos ficaram fora da janela
        assert_eq!(r.total_linhas, 10);
        assert_eq!(r.reqs, 2);
        assert_eq!(r.c5xx, 1);
        assert_eq!(r.c4xx, 0);
        assert_eq!(r.max_seg, Some(2.0));
    }

    #[test]
    fn prune_remove_somente_o_antigo() {
        let conn = banco();
        inserir_container(&conn, "app", "running", 1000);
        let mut niveis = std::collections::BTreeMap::new();
        niveis.insert("INFO".to_string(), 1usize);
        armazenar_contagens(&conn, "app", &niveis, 100).unwrap();
        armazenar_contagens(&conn, "app", &niveis, 900).unwrap();

        prune_antigos(&conn, 500).unwrap();

        let restantes: i64 = conn
            .query_row("SELECT COUNT(*) FROM log_counts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(restantes, 1);
    }

    #[test]
    fn armazenar_linhas_acumula_entre_coletas() {
        let conn = banco();
        let mut grupos = std::collections::BTreeMap::new();
        grupos.insert("INFO".to_string(), vec!["linha 1".to_string()]);
        armazenar_linhas(&conn, "app", &grupos, 100).unwrap();
        armazenar_linhas(&conn, "app", &grupos, 200).unwrap();

        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM log_lines", [], |r| r.get(0))
            .unwrap();
        // Antes esta função APAGAVA as linhas anteriores; agora acumula
        // (a retenção é por tempo, via prune_antigos).
        assert_eq!(total, 2);
    }

    #[test]
    fn carregar_linhas_janela_filtra_por_nivel_e_janela() {
        let conn = banco();
        let mut grupos = std::collections::BTreeMap::new();
        grupos.insert("ERROR".to_string(), vec!["erro novo".to_string()]);
        grupos.insert("INFO".to_string(), vec!["info nova".to_string()]);
        armazenar_linhas(&conn, "app", &grupos, 1000).unwrap();
        let mut antigos = std::collections::BTreeMap::new();
        antigos.insert("ERROR".to_string(), vec!["erro velho".to_string()]);
        armazenar_linhas(&conn, "app", &antigos, 10).unwrap();

        // Filtro por nível: só o ERROR dentro da janela (corte 500).
        let erros = carregar_linhas_janela(&conn, "app", Some("ERROR"), 500, 100).unwrap();
        assert_eq!(erros.len(), 1);
        assert_eq!(erros[0].linha, "erro novo");
        assert_eq!(erros[0].nivel, "ERROR");

        // Sem filtro: as duas linhas da janela, e nada do container errado.
        let todas = carregar_linhas_janela(&conn, "app", None, 500, 100).unwrap();
        assert_eq!(todas.len(), 2);
        let outro = carregar_linhas_janela(&conn, "outro", None, 0, 100).unwrap();
        assert!(outro.is_empty());
    }

    #[test]
    fn carregar_linhas_janela_respeita_limite_e_ordem() {
        let conn = banco();
        for (i, ts) in [(1, 100i64), (2, 200), (3, 300)] {
            let mut grupos = std::collections::BTreeMap::new();
            grupos.insert("INFO".to_string(), vec![format!("linha {i}")]);
            armazenar_linhas(&conn, "app", &grupos, ts).unwrap();
        }
        let linhas = carregar_linhas_janela(&conn, "app", None, 0, 2).unwrap();
        // Limite 2, mais recentes primeiro.
        assert_eq!(linhas.len(), 2);
        assert_eq!(linhas[0].linha, "linha 3");
        assert_eq!(linhas[1].linha, "linha 2");
    }

    #[test]
    fn alertas_recentes_ordena_e_respeita_corte() {
        let conn = banco();
        conn.execute(
            "INSERT INTO alerts (container_name, alert_type, message, created_at)
             VALUES ('app', 'stopped', 'Container parou', 100),
                    ('app', 'restarted', 'Container reiniciou', 900)",
            [],
        )
        .unwrap();

        let alertas = alertas_recentes(&conn, 500, 100).unwrap();
        assert_eq!(alertas.len(), 1);
        assert_eq!(alertas[0].tipo, "restarted");
        assert_eq!(alertas[0].container, "app");
        assert_eq!(alertas[0].criado_em, 900);

        // Corte 0 pega os dois, mais recente primeiro.
        let todos = alertas_recentes(&conn, 0, 100).unwrap();
        assert_eq!(todos.len(), 2);
        assert_eq!(todos[0].tipo, "restarted");
    }
}
