// OPERAÇÕES DE BANCO: inicialização, armazenamento de contagens/linhas,
// verificação de status de containers e exibição do acumulado.

use std::collections::BTreeMap;

use rusqlite::Connection;

use crate::core::LoguruEntry;
use crate::metricas::{ResumoContainer, intensidade_log, p95};

/// Níveis considerados "erro/crítico" nas consultas que cruzam containers
/// (`erros_desde` e `historico_por_hora`). Fonte única de verdade: antes
/// cada query tinha sua própria cópia literal desta lista em SQL — mudar um
/// nível (ex.: adicionar uma abreviação nova) exigia lembrar de editar os
/// dois lugares. `clausula_niveis_erro` monta o SQL `IN (...)` a partir
/// desta constante.
pub const NIVEIS_ERRO: [&str; 5] = ["ERROR", "ERRO", "CRIT", "CRITICAL", "FATAL"];

/// Monta a cláusula `IN ('ERROR','ERRO',...)` a partir de `NIVEIS_ERRO`,
/// pronta para ser colada num SQL literal. Os valores são constantes
/// definidas neste arquivo (não entrada do usuário), então concatenar
/// direto na string aqui não abre brecha de SQL injection — os parâmetros
/// vindos de fora continuam passando por `?N`/`params!`.
fn clausula_niveis_erro() -> String {
    let niveis: Vec<String> = NIVEIS_ERRO.iter().map(|n| format!("'{n}'")).collect();
    format!("({})", niveis.join(","))
}

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

/// Uma linha de erro/crítico carregada para o feed global — item do
/// endpoint `/api/erros`. Diferente de `LinhaLog`, inclui `id` (o cursor
/// de paginação incremental) e `container`, porque a consulta cruza
/// containers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ErroLog {
    pub id: i64,
    pub container: String,
    pub nivel: String,
    pub linha: String,
    pub collected_at: i64,
}

/// Erros/críticos de QUALQUER container para o feed global.
///
/// - `desde_id <= 0` (carga inicial): os `limite` mais recentes, devolvidos
///   em `id ASC` para o cliente posicionar o cursor no maior id do lote.
/// - `desde_id > 0` (poll incremental): só `id > desde_id`, em `id ASC`.
///
/// Cursor por `id` (PK autoincrement), não por `collected_at`: timestamps
/// podem colidir entre linhas da mesma coleta, então não servem como
/// cursor de "já vi isso". `id` é estritamente crescente.
/// `limite` protege a API de respostas gigantes se o cliente ficar muito
/// tempo sem buscar (aba aberta dias).
pub fn erros_desde(
    conn: &Connection,
    desde_id: i64,
    limite: usize,
) -> Result<Vec<ErroLog>, Box<dyn std::error::Error>> {
    // Closure local: mapeia a row nos dois SQLs (mesmo shape de colunas).
    let mapear = |row: &rusqlite::Row<'_>| -> rusqlite::Result<ErroLog> {
        Ok(ErroLog {
            id: row.get(0)?,
            container: row.get(1)?,
            nivel: row.get(2)?,
            linha: row.get(3)?,
            collected_at: row.get(4)?,
        })
    };

    let niveis = clausula_niveis_erro();
    if desde_id <= 0 {
        // Bootstrap: pega os mais novos (DESC) e inverte para ASC — o
        // cliente avança o cursor com `lote.last().id` sem caso especial.
        let mut stmt = conn.prepare(&format!(
            "SELECT id, container_name, level, line, collected_at FROM log_lines
             WHERE level IN {niveis}
             ORDER BY id DESC LIMIT ?1"
        ))?;
        let erros = stmt.query_map(rusqlite::params![limite as i64], mapear)?;
        let mut lote: Vec<ErroLog> = erros.filter_map(|r| r.ok()).collect();
        lote.reverse();
        return Ok(lote);
    }

    let mut stmt = conn.prepare(&format!(
        "SELECT id, container_name, level, line, collected_at FROM log_lines
         WHERE id > ?1 AND level IN {niveis}
         ORDER BY id ASC LIMIT ?2"
    ))?;
    let erros = stmt.query_map(rusqlite::params![desde_id, limite as i64], mapear)?;
    Ok(erros.filter_map(|r| r.ok()).collect())
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

/// Uma célula do histórico por hora: contagem de erros+críticos naquela
/// hora específica. A intensidade (0–5) é derivada no portal a partir de
/// `quantidade` (mesma escala de cores do heatmap do design Classical).
#[derive(Debug, Clone, serde::Serialize)]
pub struct CelulaHistorico {
    /// Timestamp Unix (segundos) do INÍCIO da hora (sempre múltiplo de 3600).
    pub hora: i64,
    pub quantidade: i64,
    /// Intensidade 0..=5 já calculada no servidor (escala log, relativa ao
    /// maior `quantidade` de toda a resposta) — o portal só pinta, não
    /// recalcula. Antes o cliente reimplementava essa conta com faixas
    /// fixas chumbadas na JSX (`n <= 2 ? 1 : ...`); centralizar aqui evita
    /// as duas escalas divergirem. Ver `crate::metricas::intensidade_log`,
    /// a MESMA função usada pelo heatmap mensal de custos de IA.
    pub intensidade: u8,
}

/// Linha do histórico de um container: nome + 24 células (mais recente
/// primeiro). `total` é a soma das 24 células, para o "X" à direita do strip.
#[derive(Debug, Clone, serde::Serialize)]
pub struct HistoricoContainer {
    pub nome: String,
    pub horas: Vec<CelulaHistorico>,
    pub total: i64,
}

/// Carrega, para cada container conhecido, a contagem de erros+críticos
/// agrupada por hora nas últimas `janela_horas` horas (default 24).
///
/// A consulta agrega direto em SQL (`strftime('%H', ts, 'unixepoch')`) e
/// depois "preenche" as horas faltantes com `quantidade = 0` no Rust, para
/// o portal sempre receber 24 células por container (alinhadas, índice 0 =
/// hora mais recente, índice 23 = a mais antiga da janela).
///
/// Containers sem NENHUM erro/crítico na janela também aparecem, com todas
/// as células zeradas — o strip do portal deve mostrar mesmo container
/// "limpo" para a comparação visual ficar justa.
pub fn historico_por_hora(
    conn: &Connection,
    janela_horas: i64,
    agora: i64,
) -> Result<Vec<HistoricoContainer>, Box<dyn std::error::Error>> {
    // SQL: uma linha por (container, hora_inicio) com a soma de erros+críticos.
    // (hora_inicio = floor(ts/3600)*3600) agrupa pelo INÍCIO da hora
    // (alinhado, fácil de exibir como "14h", "15h"...).
    let niveis = clausula_niveis_erro();
    let mut stmt = conn.prepare(&format!(
        "SELECT lc.container_name,
                (lc.collected_at / 3600) * 3600 AS hora_inicio,
                COALESCE(SUM(lc.count), 0) AS qtd
         FROM log_counts lc
         WHERE lc.level IN {niveis}
           AND lc.collected_at >= ?1
         GROUP BY lc.container_name, hora_inicio
         ORDER BY lc.container_name, hora_inicio DESC"
    ))?;
    let corte = agora - janela_horas * 3600;
    // Mapa: nome -> mapa hora_inicio -> qtd (preenchido pelo SQL).
    let mut por_container: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<i64, i64>,
    > = std::collections::BTreeMap::new();
    let linhas = stmt.query_map(rusqlite::params![corte], |linha| {
        let nome: String = linha.get(0)?;
        let hora: i64 = linha.get(1)?;
        let qtd: i64 = linha.get(2)?;
        Ok((nome, hora, qtd))
    })?;
    for linha in linhas {
        let (nome, hora, qtd) = linha?;
        por_container.entry(nome).or_default().insert(hora, qtd);
    }

    // Garante que TODOS os containers conhecidos apareçam, mesmo sem erros.
    let mut nomes: Vec<String> = conn
        .prepare("SELECT name FROM containers")?
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    for nome in por_container.keys() {
        if !nomes.contains(nome) {
            nomes.push(nome.clone());
        }
    }
    nomes.sort();

    // Maior `qtd` de TODAS as células (todos os containers, todas as
    // horas) — a referência "5 = pico" da escala de intensidade. Um único
    // máximo global (em vez de um por container) deixa a cor comparável
    // entre containers: "o pior momento do container A" só fica vermelho
    // se for realmente o pior de TODOS, não só do próprio A.
    let maximo: i64 = por_container
        .values()
        .flat_map(|horas| horas.values())
        .copied()
        .max()
        .unwrap_or(0);

    // Monta o output: para cada container, gera as N horas (mais recente
    // primeiro), com `quantidade = 0` nas horas sem dados.
    let mut resultado = Vec::with_capacity(nomes.len());
    for nome in nomes {
        let contagens = por_container.get(&nome).cloned().unwrap_or_default();
        let mut horas = Vec::with_capacity(janela_horas as usize);
        let mut total: i64 = 0;
        // `i` em 0..janela_horas: 0 = hora atual, janela_horas-1 = mais antiga.
        for i in 0..janela_horas {
            // Alinhado no início da hora: agora-3600*i, arredondado para
            // baixo até o múltiplo de 3600 mais próximo.
            let hora_inicio = (agora / 3600) * 3600 - i * 3600;
            let qtd = contagens.get(&hora_inicio).copied().unwrap_or(0);
            total += qtd;
            horas.push(CelulaHistorico {
                hora: hora_inicio,
                quantidade: qtd,
                intensidade: intensidade_log(qtd, maximo),
            });
        }
        resultado.push(HistoricoContainer { nome, horas, total });
    }
    Ok(resultado)
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
    fn erros_desde_filtra_nivel_cursor_limite_e_mistura_containers() {
        let conn = banco();
        // Ordem de inserção = ordem de id (autoincrement). Mistura níveis
        // e containers para garantir o filtro e o cruzamento.
        conn.execute(
            "INSERT INTO log_lines (container_name, level, line, collected_at)
             VALUES ('app', 'INFO', 'ok', 100),
                    ('app', 'ERROR', 'erro app', 100),
                    ('zen', 'CRITICAL', 'crit zen', 100),
                    ('app', 'WARNING', 'aviso', 100),
                    ('zen', 'FATAL', 'fatal zen', 100),
                    ('app', 'ERRO', 'erro pt', 100)",
            [],
        )
        .unwrap();

        // desde_id 0: só ERROR/ERRO/CRIT/CRITICAL/FATAL, id ASC, todos os containers.
        let todos = erros_desde(&conn, 0, 100).unwrap();
        assert_eq!(todos.len(), 4);
        assert_eq!(todos[0].container, "app");
        assert_eq!(todos[0].nivel, "ERROR");
        assert_eq!(todos[0].linha, "erro app");
        assert_eq!(todos[1].container, "zen");
        assert_eq!(todos[1].nivel, "CRITICAL");
        assert_eq!(todos[2].nivel, "FATAL");
        assert_eq!(todos[3].nivel, "ERRO");

        // Cursor: ids > primeiro erro — não repete o que já passou.
        let apos = erros_desde(&conn, todos[0].id, 100).unwrap();
        assert_eq!(apos.len(), 3);
        assert_eq!(apos[0].id, todos[1].id);

        // Limite na carga inicial: os N mais recentes (não os mais antigos).
        let um = erros_desde(&conn, 0, 1).unwrap();
        assert_eq!(um.len(), 1);
        assert_eq!(um[0].id, todos[3].id);
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

    #[test]
    fn historico_por_hora_preenche_lacunas_e_inclui_container_limpo() {
        // agora = 100_000 (escolhido para alinhar com hora cheia: 100_000 /
        // 3600 = 27, resto 2800). 24h de janela cobre ts >= 100_000 - 24*3600
        // = 13_600. Containers: "ruim" com erros em 2 horas distintas;
        // "limpo" registrado mas sem NENHUMA contagem na janela.
        let agora = 100_000i64;
        let janela = 24i64;
        let conn = banco();
        inserir_container(&conn, "ruim", "running", agora);
        inserir_container(&conn, "limpo", "running", agora);

        // 3 ERRORs há 1h (hora atual, contagem grande) e 5 CRITICALs há 23h
        // (cabe na janela de 24h, fica na última posição do strip).
        let mut niveis = std::collections::BTreeMap::new();
        niveis.insert("ERROR".to_string(), 3usize);
        armazenar_contagens(&conn, "ruim", &niveis, agora - 3600).unwrap();
        let mut antigos = std::collections::BTreeMap::new();
        antigos.insert("CRITICAL".to_string(), 5usize);
        armazenar_contagens(&conn, "ruim", &antigos, agora - 23 * 3600).unwrap();

        // INFO não conta (só ERROR/ERRO/CRIT/CRITICAL/FATAL).
        let mut info = std::collections::BTreeMap::new();
        info.insert("INFO".to_string(), 100usize);
        armazenar_contagens(&conn, "ruim", &info, agora - 5 * 3600).unwrap();

        // Contagem FORA da janela: não deve aparecer.
        let mut fora = std::collections::BTreeMap::new();
        fora.insert("ERROR".to_string(), 99usize);
        armazenar_contagens(&conn, "ruim", &fora, agora - janela * 3600 - 1).unwrap();

        let hist = historico_por_hora(&conn, janela, agora).unwrap();
        // Os DOIS containers aparecem, mesmo o "limpo".
        assert_eq!(hist.len(), 2);
        let ruim = hist.iter().find(|h| h.nome == "ruim").unwrap();
        let limpo = hist.iter().find(|h| h.nome == "limpo").unwrap();

        // Strip sempre tem `janela` células, mais recente primeiro.
        assert_eq!(ruim.horas.len(), janela as usize);
        assert_eq!(limpo.horas.len(), janela as usize);

        // 1ª célula = hora ATUAL (sem contagens novas — as 3 estão em
        // agora-3600, que cai na hora ANTERIOR, célula de índice 1).
        assert_eq!(ruim.horas[0].quantidade, 0);
        // 2ª célula (hora anterior) = 3 (os 3 ERRORs agora-3600).
        assert_eq!(ruim.horas[1].quantidade, 3);
        // Última célula (23h atrás) do "ruim" = 5 (os 5 CRITICALs).
        assert_eq!(ruim.horas[23].quantidade, 5);
        // Demais: zero (INFO não conta; contagem fora da janela não conta).
        assert!(ruim.horas[2..23].iter().all(|c| c.quantidade == 0));
        // Total = 3 + 5 (a de fora não entra).
        assert_eq!(ruim.total, 8);

        // "limpo" não tem NENHUMA célula preenchida.
        assert!(limpo.horas.iter().all(|c| c.quantidade == 0));
        assert_eq!(limpo.total, 0);

        // Alinhamento: `hora` de cada célula é múltiplo de 3600.
        for c in &ruim.horas {
            assert_eq!(c.hora % 3600, 0);
        }

        // Intensidade: a célula com o maior valor do conjunto todo (os 5
        // CRITICALs) fica no teto (5); célula sem contagem fica 0.
        assert_eq!(ruim.horas[23].intensidade, 5);
        assert_eq!(ruim.horas[0].intensidade, 0);
        assert!(limpo.horas.iter().all(|c| c.intensidade == 0));
    }
}
