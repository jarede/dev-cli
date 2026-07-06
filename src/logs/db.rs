// OPERAÇÕES DE BANCO: inicialização, armazenamento de contagens/linhas,
// verificação de status de containers e exibição do acumulado.

use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::logs::render::renderizar_container;

/// Cria as tabelas do banco se não existirem e executa migrações.
// docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.execute_batch
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
pub(crate) fn armazenar_contagens(
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
/// Remove linhas antigas do mesmo container para evitar acúmulo infinito.
pub(crate) fn armazenar_linhas(
    conn: &Connection,
    nome: &str,
    grupos: &BTreeMap<String, Vec<String>>,
    agora: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Remove linhas antigas deste container (mantém só as últimas coletas)
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

/// Lê as contagens acumuladas do banco e formata para exibição.
pub(crate) fn exibir_estatisticas(conn: &Connection) -> Result<String, Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT container_name, level, SUM(count) as total
         FROM log_counts
         GROUP BY container_name, level
         ORDER BY container_name, level",
    )?;

    // Mapa aninhado: container -> (nível -> total). O `BTreeMap` externo dá
    // ordem alfabética por container; o interno, por nível.
    let mut dados: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    // O closure de `query_map` roda por linha e pode falhar em cada `row.get`
    // (tipo errado, coluna ausente); por isso ele próprio devolve `Result`,
    // e usamos `?` dentro dele para propagar esse erro por linha.
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Statement.html#method.query_map
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Row.html#method.get
    let linhas = stmt.query_map([], |row| {
        let nome: String = row.get(0)?;
        let nivel: String = row.get(1)?;
        let total: i64 = row.get(2)?;
        Ok((nome, nivel, total as usize))
    })?;

    for linha in linhas {
        // Aqui o `?` é sobre o `Result` de CADA linha (o iterador de
        // `query_map` produz `Result<(...), Error>`), não sobre o closure
        // acima.
        let (nome, nivel, total) = linha?;
        // API `entry`: garante um `BTreeMap` vazio para containers novos
        // antes de inserir o par nível/total.
        // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.entry
        // docs: https://doc.rust-lang.org/std/collections/btree_map/enum.Entry.html#method.or_default
        // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.insert
        dados.entry(nome).or_default().insert(nivel, total);
    }

    // Carrega o status (uptime) de cada container do banco
    let mut stmt2 = conn
        .prepare("SELECT name, uptime FROM containers WHERE uptime IS NOT NULL AND uptime != ''")?;
    let mut status_map: BTreeMap<String, String> = BTreeMap::new();
    // `.flatten()` sobre um iterador de `Result<(String, String), Error>`
    // funciona porque `Result` também implementa `IntoIterator` (0 ou 1
    // item): `Ok(x)` vira um iterador de 1 elemento, `Err(_)` vira vazio —
    // é uma forma mais curta de "ignore os erros e siga com os `Ok`".
    // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.flatten
    // docs: https://doc.rust-lang.org/std/result/enum.Result.html
    // docs: https://doc.rust-lang.org/std/iter/trait.IntoIterator.html
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
