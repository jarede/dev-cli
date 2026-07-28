// Endpoint de IA · custos: lê o SQLite do OpenCode (se existir) e devolve
// os mesmos dados que `dev-cli ai stats opencode --json` produz, mais as
// horas trabalhadas com Claude Code (lidas dos transcritos JSONL locais —
// mesma fonte de `dev-cli ai stats claude`), empacotados para o portal web.
//
// Duas fontes independentes, duas disponibilidades independentes: o
// OpenCode pode ter dados sem o Claude Code ter sessões no mês (ou
// vice-versa) — por isso `disponivel` (OpenCode) e `claude_disponivel`
// (Claude Code) são flags separadas, não uma só.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use axum::Json;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use nucleo::horas_sessao::{self, Sessao};

use crate::api::EstadoApi;

/// Caminho padrão do banco do OpenCode (mesmo do CLI). Permite override
/// pela env `DEV_CLI_OPENCODE_DB` — útil em testes que querem forçar o
/// caminho "banco inexistente" para exercitar a resposta vazia.
fn caminho_db_opencode() -> PathBuf {
    if let Ok(c) = std::env::var("DEV_CLI_OPENCODE_DB") {
        return PathBuf::from(c);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".local/share/opencode/opencode.db")
}

/// Diretório dos transcritos do Claude Code (mesmo do CLI, ver
/// `crates/cli/src/ai/claude.rs::diretorio_projetos`). Permite override
/// pela env `DEV_CLI_CLAUDE_PROJETOS_DIR` — mesmo padrão de
/// `DEV_CLI_OPENCODE_DB`, para os testes forçarem "sem sessões" sem
/// depender do que existir na máquina de quem roda `cargo test`.
fn diretorio_projetos_claude() -> PathBuf {
    if let Ok(c) = std::env::var("DEV_CLI_CLAUDE_PROJETOS_DIR") {
        return PathBuf::from(c);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".claude/projects")
}

/// Query string de `/api/ia/custos`: `?mes=YYYY-MM`. Default = mês atual.
#[derive(Deserialize)]
pub struct ParamsCustos {
    pub mes: Option<String>,
}

/// Estrutura completa da resposta JSON. Campos derivados calculados no
/// Rust (streak, troca de mês) ficam junto dos brutos do banco.
#[derive(Serialize)]
pub struct CustosIa {
    /// "YYYY-MM" resolvido (default = mês atual em horário local).
    pub mes: String,
    /// True se o banco do OpenCode foi lido com sucesso; false = sem dados.
    pub disponivel: bool,
    /// Total de tokens no mês (entrada + saída + reasoning + cache).
    pub tokens: i64,
    /// Custo estimado no mês em USD.
    pub custo_usd: f64,
    /// Porcentagem de tokens que vieram de cache (read+write) sobre o total.
    /// Útil para o "86% em cache" da UI.
    pub cache_pct: f64,
    /// Streak: dias consecutivos com tokens > 0 terminando HOJE.
    pub streak_dias: i64,
    /// Melhor streak dentro do mês (para o "melhor: 23 dias").
    pub melhor_streak_dias: i64,
    /// Heatmap do mês: uma célula por dia do mês (1..=31). `null` = sem
    /// dados naquele dia (mantém o tipo simples; a UI usa `null` para
    /// "transparente" no heatmap).
    pub heatmap: Vec<CelulaHeatmap>,
    /// Dia da semana do dia 1º do mês (0 = segunda ... 6 = domingo) — o
    /// offset de células transparentes que a UI precisa desenhar ANTES da
    /// célula do dia 1 para o heatmap alinhar com os rótulos seg/qua/sex/dom
    /// (achado 6 da revisão: antes esse offset estava chumbado em 0 no
    /// React, fazendo o dia 1 sempre cair na linha "seg").
    pub offset_semana_dia1: u32,
    /// Lista de modelos usados no mês (ranking por tokens DESC).
    pub modelos: Vec<ModeloCusto>,
    /// True se havia pelo menos uma sessão do Claude Code no mês (fonte:
    /// transcritos JSONL em `~/.claude/projects`, INDEPENDENTE do banco do
    /// OpenCode acima) — `false` faz a UI mostrar um estado vazio honesto
    /// em vez de "—" fingindo que o dado existe.
    pub claude_disponivel: bool,
    /// Total de horas trabalhadas com Claude Code no mês (soma da duração
    /// de todas as sessões, mesmo clamp de `dev-cli ai stats claude`).
    pub claude_horas_mes: f64,
    /// Média de horas por DIA ATIVO (dias com pelo menos 1 sessão) — não
    /// por dia do mês inteiro, que penalizaria fins de semana sem uso.
    pub claude_media_horas_dia_ativo: f64,
    /// Horas agregadas por semana (segunda a segunda) — as linhas da seção
    /// "Horas por semana" da tela IA · custos.
    pub claude_horas_por_semana: Vec<SemanaHoras>,
}

/// Uma célula do heatmap: dia do mês + intensidade (0 = sem dados).
#[derive(Serialize)]
pub struct CelulaHeatmap {
    pub dia: u32,
    pub intensidade: u8,
}

/// Uma semana no gráfico "Horas por semana": rótulo (a segunda-feira da
/// semana, "dd/mm") + total de horas.
#[derive(Serialize)]
pub struct SemanaHoras {
    pub rotulo: String,
    pub horas: f64,
}

/// Um modelo + seus totais no mês.
#[derive(Serialize)]
pub struct ModeloCusto {
    pub modelo: String,
    pub provedor: String,
    pub sessoes: i64,
    pub tokens: i64,
    pub custo_usd: f64,
}

/// GET /api/ia/custos — pacote completo para a tela IA · custos do portal.
/// Se o banco do OpenCode não existe ou falha, devolve `disponivel: false`
/// e zeros — o portal mostra o estado "sem dados" sem quebrar.
pub async fn custos(
    // `_estado` está aqui só para casar com a assinatura exigida pela rota
    // (precisa ter `State<EstadoApi>` para o `.with_state` aplicar); os
    // dados do OpenCode moram em OUTRO banco (não no do dev-server), então
    // não consultamos `estado.db` aqui.
    State(_estado): State<EstadoApi>,
    Query(params): Query<ParamsCustos>,
) -> Result<Json<CustosIa>, (StatusCode, String)> {
    let mes = params
        .mes
        .unwrap_or_else(|| Local::now().format("%Y-%m").to_string());

    // DB ausente OU falhou de abrir: parte da resposta "vazia" em vez de
    // 500 — a UI mostra o estado vazio sem piscar erro vermelho.
    let mut resultado = match abrir_opencode() {
        Some(conn) => match agregar_opencode(&conn, &mes) {
            Ok(mut agreg) => {
                // Streak é derivado do heatmap (a UI poderia calcular, mas
                // fazer no servidor mantém o cliente burro).
                agreg.streak_dias = calcular_streak(&agreg.heatmap, 0);
                // `melhor_streak_dias` = maior sequência de dias não-zero
                // dentro do mês.
                agreg.melhor_streak_dias = calcular_melhor_streak(&agreg.heatmap);
                agreg
            }
            Err(_) => resposta_vazia(&mes),
        },
        None => resposta_vazia(&mes),
    };

    // Câmpos independentes do OpenCode: offset do heatmap (sempre
    // calculável a partir só do mês) e horas do Claude Code (fonte
    // completamente separada — JSONL local, não o SQLite acima).
    resultado.offset_semana_dia1 = offset_semana_dia1(&mes);
    let (disponivel, horas_mes, media_dia_ativo, por_semana) = calcular_horas_claude(&mes);
    resultado.claude_disponivel = disponivel;
    resultado.claude_horas_mes = horas_mes;
    resultado.claude_media_horas_dia_ativo = media_dia_ativo;
    resultado.claude_horas_por_semana = por_semana;

    Ok(Json(resultado))
}

/// Tenta abrir o banco do OpenCode; devolve None se não existe (ou se
/// falhou). Silencioso: o portal não precisa saber por que faltou.
fn abrir_opencode() -> Option<Connection> {
    let caminho = caminho_db_opencode();
    if !caminho.exists() {
        return None;
    }
    Connection::open(&caminho).ok()
}

/// Resposta "sem dados": todos os campos em zero, `disponivel: false`.
/// Usada quando o banco está ausente ou a query falha — a UI usa `disponivel`
/// para mostrar a mensagem "Sem dados do OpenCode" no lugar do dashboard.
fn resposta_vazia(mes: &str) -> CustosIa {
    CustosIa {
        mes: mes.to_string(),
        disponivel: false,
        tokens: 0,
        custo_usd: 0.0,
        cache_pct: 0.0,
        streak_dias: 0,
        melhor_streak_dias: 0,
        heatmap: Vec::new(),
        // Preenchidos pelo caller (`custos`), independente do OpenCode.
        offset_semana_dia1: 0,
        modelos: Vec::new(),
        claude_disponivel: false,
        claude_horas_mes: 0.0,
        claude_media_horas_dia_ativo: 0.0,
        claude_horas_por_semana: Vec::new(),
    }
}

/// Carrega os agregados do mês a partir do banco já aberto. Mesma
/// estratégia de `ai stats opencode --json` (CLI): tokens por dia, modelos
/// e totais — só sem a parte de sessões detalhadas (a UI não precisa).
fn agregar_opencode(conn: &Connection, mes: &str) -> rusqlite::Result<CustosIa> {
    // Total de tokens e "cache %" no mês. Um único query_row com SUM
    // explícito por componente — não dependemos da coluna `cost` da
    // tabela `session` aqui porque queremos separar cache do total.
    // docs: https://www.sqlite.org/lang_aggfunc.html
    let (tokens_total, cache_total): (i64, i64) = conn.query_row(
        "SELECT
            COALESCE(SUM(
                COALESCE(CAST(json_extract(data, '$.tokens.input') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.output') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.reasoning') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.cache.read') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.cache.write') AS INTEGER), 0)
            ), 0) AS total,
            COALESCE(SUM(
                COALESCE(CAST(json_extract(data, '$.tokens.cache.read') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.cache.write') AS INTEGER), 0)
            ), 0) AS cache
         FROM message
         WHERE json_extract(data, '$.role') = 'assistant'
           AND (?1 = '' OR strftime('%Y-%m', CAST(json_extract(data, '$.time.created') AS INTEGER) / 1000, 'unixepoch', 'localtime') = ?1)",
        rusqlite::params![mes],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let cache_pct = if tokens_total > 0 {
        (cache_total as f64) * 100.0 / (tokens_total as f64)
    } else {
        0.0
    };

    // Custo total: soma simples de `session.cost` no mês.
    let custo_usd: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(cost), 0) FROM session
             WHERE (?1 = '' OR strftime('%Y-%m', time_created / 1000, 'unixepoch', 'localtime') = ?1)",
            rusqlite::params![mes],
            |r| r.get(0),
        )?;

    // Heatmap: tokens por dia, mapeados para uma intensidade 0..=5 em
    // escala log (mesma curva do protótipo .dc.html). Dia SEM entrada =
    // intensidade 0 (a UI pinta com `neutral-200`).
    let mut stmt = conn.prepare(
        "SELECT date(CAST(json_extract(data, '$.time.created') AS INTEGER) / 1000, 'unixepoch', 'localtime') AS dia,
                SUM(
                    COALESCE(CAST(json_extract(data, '$.tokens.input') AS INTEGER), 0) +
                    COALESCE(CAST(json_extract(data, '$.tokens.output') AS INTEGER), 0) +
                    COALESCE(CAST(json_extract(data, '$.tokens.reasoning') AS INTEGER), 0) +
                    COALESCE(CAST(json_extract(data, '$.tokens.cache.read') AS INTEGER), 0) +
                    COALESCE(CAST(json_extract(data, '$.tokens.cache.write') AS INTEGER), 0)
                ) AS tokens
         FROM message
         WHERE json_extract(data, '$.role') = 'assistant'
           AND (?1 = '' OR strftime('%Y-%m', CAST(json_extract(data, '$.time.created') AS INTEGER) / 1000, 'unixepoch', 'localtime') = ?1)
         GROUP BY dia"
    )?;
    let mut tokens_por_dia: std::collections::BTreeMap<NaiveDate, i64> =
        std::collections::BTreeMap::new();
    let linhas = stmt.query_map(rusqlite::params![mes], |r| {
        let dia: String = r.get(0)?;
        let t: i64 = r.get(1)?;
        Ok((dia, t))
    })?;
    for linha in linhas {
        let (dia_texto, t) = linha?;
        if let Ok(d) = NaiveDate::parse_from_str(&dia_texto, "%Y-%m-%d") {
            tokens_por_dia.insert(d, t);
        }
    }
    let heatmap = montar_heatmap(&tokens_por_dia, mes);

    // Modelos do mês (ranking por tokens DESC).
    let mut stmt = conn.prepare(
        "SELECT
            json_extract(model, '$.id') AS modelo,
            COALESCE(json_extract(model, '$.providerID'), 'desconhecido') AS provedor,
            COUNT(*) AS sessoes,
            COALESCE(SUM(tokens_input + tokens_output + tokens_reasoning + tokens_cache_write + tokens_cache_read), 0) AS tokens,
            COALESCE(SUM(cost), 0) AS custo
         FROM session
         WHERE model IS NOT NULL
           AND (?1 = '' OR strftime('%Y-%m', time_created / 1000, 'unixepoch', 'localtime') = ?1)
         GROUP BY modelo, provedor
         ORDER BY tokens DESC"
    )?;
    let mut modelos: Vec<ModeloCusto> = Vec::new();
    let linhas = stmt.query_map(rusqlite::params![mes], |r| {
        Ok(ModeloCusto {
            modelo: r.get(0)?,
            provedor: r.get(1)?,
            sessoes: r.get(2)?,
            tokens: r.get(3)?,
            custo_usd: r.get(4)?,
        })
    })?;
    for m in linhas {
        modelos.push(m?);
    }

    Ok(CustosIa {
        mes: mes.to_string(),
        disponivel: true,
        tokens: tokens_total,
        custo_usd,
        cache_pct,
        streak_dias: 0,        // preenchido pelo caller
        melhor_streak_dias: 0, // preenchido pelo caller
        heatmap,
        offset_semana_dia1: 0, // preenchido pelo caller
        modelos,
        claude_disponivel: false,            // preenchido pelo caller
        claude_horas_mes: 0.0,               // preenchido pelo caller
        claude_media_horas_dia_ativo: 0.0,   // preenchido pelo caller
        claude_horas_por_semana: Vec::new(), // preenchido pelo caller
    })
}

/// Monta o heatmap de um mês: para cada dia 1..=último, intensidade 0..=5
/// baseada em log dos tokens (escala igual à do protótipo .dc.html — "0 =
/// sem atividade", "5 = muito intenso"). Dias futuros ou fora do mês não
/// aparecem; a UI infere a posição via `dia` (1-based) e a data real.
fn montar_heatmap(
    tokens_por_dia: &std::collections::BTreeMap<NaiveDate, i64>,
    mes: &str,
) -> Vec<CelulaHeatmap> {
    // Parse "YYYY-MM" para saber quantos dias tem o mês.
    let Ok(primeiro) = NaiveDate::parse_from_str(&format!("{mes}-01"), "%Y-%m-%d") else {
        return Vec::new();
    };
    // chrono não expõe `days_in_month` diretamente; calculamos somando 1
    // mês e subtraindo 1 dia. O `unwrap` é seguro: o ano está validado
    // pelo parse acima.
    let ultimo = if primeiro.month() == 12 {
        NaiveDate::from_ymd_opt(primeiro.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(primeiro.year(), primeiro.month() + 1, 1)
    }
    .and_then(|d| d.pred_opt())
    .unwrap_or(primeiro);

    // Limite superior da intensidade: o maior valor do mês (define o "5").
    // Se ninguém trabalhou, fica tudo 0.
    let max_tokens = tokens_por_dia.values().copied().max().unwrap_or(0);

    let mut heatmap = Vec::with_capacity(ultimo.day() as usize);
    for d in 1..=ultimo.day() {
        let dia = NaiveDate::from_ymd_opt(primeiro.year(), primeiro.month(), d);
        let intensidade = match dia {
            Some(data) => nucleo::metricas::intensidade_log(
                tokens_por_dia.get(&data).copied().unwrap_or(0),
                max_tokens,
            ),
            None => 0,
        };
        heatmap.push(CelulaHeatmap {
            dia: d,
            intensidade,
        });
    }
    heatmap
}

// Mapeamento tokens -> intensidade 0..=5: `nucleo::metricas::intensidade_log`
// (mesma fórmula usada pelo histórico de erros por hora — ver o achado de
// unificação em `db::historico_por_hora`). Antes esta função tinha uma
// cópia local idêntica; centralizar evita as duas escalas divergirem.

/// Dia da semana do dia 1º do mês: 0 = segunda ... 6 = domingo —
/// `num_days_from_monday` já devolve exatamente essa convenção. É o offset
/// de células vazias/transparentes que o heatmap da UI precisa desenhar
/// ANTES da célula do dia 1 para a coluna bater com a linha certa
/// (seg/qua/sex/dom). Mês inválido (não deveria acontecer — `mes` vem de
/// `Local::now()` ou de uma query já validada) cai em 0 em vez de falhar
/// o endpoint inteiro por causa de um detalhe visual.
fn offset_semana_dia1(mes: &str) -> u32 {
    NaiveDate::parse_from_str(&format!("{mes}-01"), "%Y-%m-%d")
        .map(|d| d.weekday().num_days_from_monday())
        .unwrap_or(0)
}

/// Uma linha crua do JSONL do Claude Code — só os dois campos que
/// interessam para calcular DURAÇÃO de sessão (dia + horário). Comparar
/// com `crates/cli/src/ai/claude.rs::Registro`, que também lê `message`
/// (para tokens/modelo) — aqui não precisamos disso, então nem
/// desserializamos (campo ausente na struct = serde ignora silenciosamente).
#[derive(Deserialize)]
struct RegistroClaude {
    timestamp: String,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
}

/// Lê todos os `.jsonl` sob `~/.claude/projects`, filtra pelo mês pedido e
/// devolve uma `Sessao` (dia + duração em horas) por `sessionId` — a MESMA
/// lógica de agrupamento de `crates/cli/src/ai/claude.rs::carregar_sessoes`,
/// mas sem a parte de tokens/modelo (a tela IA · custos já mostra os custos
/// via OpenCode; duplicar a tabela de preços do Claude aqui ficaria fora do
/// escopo deste endpoint). `nucleo::horas_sessao::duracao_sessao` é quem
/// aplica o clamp entre `MINIMO_HORAS` e `TETO_HORAS`.
fn carregar_sessoes_claude(mes: &str) -> Vec<Sessao> {
    let mut horarios_por_sessao: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();

    let arquivos = walkdir::WalkDir::new(diretorio_projetos_claude())
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entrada| entrada.path().extension().is_some_and(|ext| ext == "jsonl"));

    for entrada in arquivos {
        let Ok(conteudo) = std::fs::read_to_string(entrada.path()) else {
            continue;
        };
        for linha in conteudo.lines() {
            let Ok(registro) = serde_json::from_str::<RegistroClaude>(linha) else {
                continue;
            };
            if !mes.is_empty() && !registro.timestamp.starts_with(mes) {
                continue;
            }
            let Some(session_id) = registro.session_id else {
                continue;
            };
            let Ok(instante) = DateTime::parse_from_rfc3339(&registro.timestamp) else {
                continue;
            };
            horarios_por_sessao
                .entry(session_id)
                .or_default()
                .push(instante.with_timezone(&Utc));
        }
    }

    horarios_por_sessao
        .into_values()
        .filter_map(|mut horarios| {
            horarios.sort();
            let duracao_horas = horas_sessao::duracao_sessao(&horarios)?;
            let dia = horarios.first()?.with_timezone(&Local).date_naive();
            Some(Sessao { dia, duracao_horas })
        })
        .collect()
}

/// Agrega as sessões do Claude Code do mês em: disponibilidade, total de
/// horas, média por dia ativo e horas por semana. `disponivel = false`
/// quando não há NENHUMA sessão no mês — a UI mostra um estado vazio
/// honesto em vez de "—" fingindo que o dado só ainda não carregou.
fn calcular_horas_claude(mes: &str) -> (bool, f64, f64, Vec<SemanaHoras>) {
    let sessoes = carregar_sessoes_claude(mes);
    if sessoes.is_empty() {
        return (false, 0.0, 0.0, Vec::new());
    }

    let horas_mes: f64 = sessoes.iter().map(|s| s.duracao_horas).sum();
    let por_dia: BTreeMap<NaiveDate, (f64, u32)> = horas_sessao::agregar_por_dia(&sessoes);
    // `por_dia.len()` nunca é 0 aqui: `sessoes` não está vazio, então pelo
    // menos um dia tem entrada no mapa.
    let media_por_dia_ativo = horas_mes / por_dia.len() as f64;

    let semanas = horas_sessao::agregar_por_semana(&sessoes)
        .into_iter()
        .map(|(segunda, (horas, _sessoes, _dias))| SemanaHoras {
            rotulo: segunda.format("%d/%m").to_string(),
            horas,
        })
        .collect();

    (true, horas_mes, media_por_dia_ativo, semanas)
}

/// Streak atual: dias consecutivos com intensidade > 0 terminando HOJE.
/// `hoje_dia` injetado pelos testes (0 = usar data real).
fn calcular_streak(heatmap: &[CelulaHeatmap], hoje_dia: u32) -> i64 {
    // Determina "hoje" no contexto: se o caller passou 0, usa o dia real
    // (mas só se o mês da resposta bater com o mês atual — fora isso, zera).
    let hoje = if hoje_dia > 0 {
        hoje_dia
    } else {
        let now = Local::now().date_naive();
        // Heatmap é sempre de UM mês; se o mês do heatmap não é o mês
        // atual, o "hoje" pode nem estar na lista — considera streak 0
        // para evitar fingir um streak que não existe mais.
        // O caller passa o mês via heatmap (ordenado por dia), então
        // conferimos: o último `dia` do heatmap deve ser HOJE.
        // Heurística simples: se o ÚLTIMO elemento tiver dia == dia de hoje,
        // é o mês atual. Senão, sem streak.
        if heatmap.last().map(|c| c.dia) != Some(now.day()) {
            return 0;
        }
        now.day()
    };

    let mut streak = 0i64;
    for c in heatmap.iter().rev() {
        if c.dia > hoje {
            continue;
        }
        if c.intensidade > 0 {
            streak += 1;
        } else if c.dia == hoje {
            // Hoje sem atividade ainda: streak = 0 (não "ontem" como
            // GitHub faz — nossa UI mostra "X dias" e 0 é honesto).
            return 0;
        } else {
            break;
        }
    }
    streak
}

/// Maior sequência de dias não-zero dentro do mês (para o "melhor: 23").
fn calcular_melhor_streak(heatmap: &[CelulaHeatmap]) -> i64 {
    let mut melhor = 0i64;
    let mut atual = 0i64;
    for c in heatmap {
        if c.intensidade > 0 {
            atual += 1;
            if atual > melhor {
                melhor = atual;
            }
        } else {
            atual = 0;
        }
    }
    melhor
}

/// Câmbio USD → BRL para a UI. Fallback usado quando a busca ao vivo falha
/// ou estoura o timeout — mantém a tela funcionando com um valor aproximado
/// em vez de quebrar a requisição.
const CAMBIO_FALLBACK: f64 = 5.42;

#[derive(Serialize)]
pub struct Cambio {
    pub usd_brl: f64,
}

/// GET /api/ia/cambio — taxa USD→BRL ao vivo, reaproveitando
/// `nucleo::cambio::buscar_taxa_usd_brl` (a mesma busca do `dev-cli ai
/// stats`, que já falava com a API de câmbio — antes este endpoint tinha
/// uma cópia hardcoded do valor, apesar do comentário dizer que a busca
/// "já existe").
///
/// `buscar_taxa_usd_brl` é síncrona/bloqueante (usa `reqwest::blocking`
/// por baixo dos panos): chamá-la direto aqui travaria a thread do tokio
/// que está atendendo ESTA requisição, atrasando QUALQUER outra requisição
/// escalada pra mesma thread do runtime enquanto o HTTP não responde.
/// `spawn_blocking` move a chamada para o pool de threads dedicado a
/// trabalho bloqueante do tokio, liberando o executor async imediatamente.
/// Falha (rede fora, timeout, `spawn_blocking` que não retorna) sempre cai
/// no fallback — a tela de custos não deve quebrar por causa do câmbio.
/// docs: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html
pub async fn cambio() -> Json<Cambio> {
    // `buscar_taxa_usd_brl` devolve `Result<f64, Box<dyn Error>>`, e
    // `Box<dyn Error>` não implementa `Send` — não pode atravessar a
    // fronteira de thread do `spawn_blocking` direto. `.ok()` aqui DENTRO
    // do closure descarta o erro (não precisamos do texto, só de saber que
    // falhou) e vira um `Option<f64>`, que é `Send` — daí dar pra
    // `spawn_blocking` devolver.
    let taxa = tokio::task::spawn_blocking(|| nucleo::cambio::buscar_taxa_usd_brl().ok())
        .await
        .ok()
        .flatten()
        .unwrap_or(CAMBIO_FALLBACK);
    Json(Cambio { usd_brl: taxa })
}
