// Subcomando `ai stats opencode`: dashboard de tokens/custo/uso do OpenCode,
// lido direto do SQLite local do app (~/.local/share/opencode/opencode.db).
//
// Responsabilidades:
//   1. Abrir o banco SQLite
//   2. Rodar queries para extrair resumo, tokens/dia, modelos, sessões
//   3. Filtrar por mês (se o usuário passou `2026-06`)
//   4. Delegar a renderização do texto final para `render::renderizar_dashboard`
//
// A separação casca-de-IO / núcleo-puro segue o padrão de `src/logs.rs`:
// as funções `carregar_*` são IO (tocam banco), `renderizar_dashboard` é
// pura (só transforma dados em string).

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{Local, NaiveDate};
use clap::Args;
use rusqlite::Connection;

use crate::ai::render;

/// Estatísticas de uso do OpenCode a partir do banco SQLite local.
///
/// `#[derive(Args)]` é a macro do `clap` que lê os atributos `#[arg(...)]`
/// de cada campo abaixo e gera, em tempo de compilação, o parser de linha de
/// comando (flags, posicionais, valores default, validação de range etc.) —
/// não escrevemos esse parsing na mão. `Debug` só permite formatar a struct
/// com `{:?}`, útil em prints de depuração.
#[derive(Args, Debug)]
#[command(
    help_template = crate::help::ARGUMENTOS,
    next_help_heading = crate::help::OPCOES
)]
pub struct OpencodeArgs {
    /// Período: mês (YYYY-MM) ou dia (YYYY-MM-DD). Se omitido, usa o mês atual.
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    periodo: Option<String>,

    /// Mostra todo o histórico (ignora o filtro de período).
    #[arg(long, conflicts_with = "periodo", help_heading = crate::help::OPCOES)]
    historico: bool,

    /// Caminho customizado do banco SQLite do OpenCode.
    #[arg(long, help_heading = crate::help::OPCOES)]
    db: Option<PathBuf>,

    /// Largura do heatmap em semanas (4-104, padrão 52).
    #[arg(long, default_value_t = 52, value_parser = clap::value_parser!(u32).range(4..=104), help_heading = crate::help::OPCOES)]
    weeks: u32,

    /// Desativa cores ANSI (útil para pipes/arquivos).
    #[arg(long, help_heading = crate::help::OPCOES)]
    no_color: bool,

    /// Em vez do dashboard, imprime JSON com os dados brutos.
    #[arg(long, help_heading = crate::help::OPCOES)]
    json: bool,
}

// ── Resumo agregado ─────────────────────────────────────────────────
// Guarda os totais de tarefas e tokens no período consultado. O custo
// total agora é calculado a partir dos modelos (já com estimativas -go),
// não deste resumo.
//
// Struct só de uso interno (sem `pub`, sem derive): existe apenas para dar
// nome aos dois valores que `carregar_resumo` devolve, em vez de retornar
// uma tupla `(i64, i64)` anônima — deixa o código de quem consome mais
// legível (`resumo.tarefas` em vez de `resumo.0`).
struct Resumo {
    /// Quantidade de mensagens do assistant no período (cada uma conta
    /// como uma "tarefa" concluída).
    tarefas: i64,
    /// Soma de todos os tokens (entrada + saída + reasoning + cache) no
    /// período, já agregada pela query SQL.
    tokens_totais: i64,
}

// ── Helpers ─────────────────────────────────────────────────────────
// Caminho padrão do banco SQLite do OpenCode. A variável `HOME` deve
// estar definida em sistemas Unix; se não estiver (shell quebrado),
// usamos "." como fallback para gerar um caminho relativo.
fn caminho_padrao_db() -> PathBuf {
    // `std::env::var` devolve `Result<String, VarError>` (falha se a variável
    // não existir ou não for UTF-8 válido). `unwrap_or_else` com uma closure
    // só roda o fallback (`".".to_string()`) se der erro — evita alocar a
    // string default no caminho feliz.
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    // `PathBuf::join` concatena componentes de caminho de forma portável
    // (insere o separador certo por sistema operacional).
    PathBuf::from(home).join(".local/share/opencode/opencode.db")
}

// ── Filtro de período ───────────────────────────────────────────────
// Fragmento SQL reaproveitado nas queries que precisam filtrar por
// período: `periodo` vazio ("") desliga o filtro (mostra tudo — usado
// pelo `--historico`); com 7 caracteres ("2026-06") filtra por mês;
// com 10 ("2026-06-02") filtra pelo dia exato. O comprimento da string
// decide o formato, dispensando um segundo argumento — mesmo padrão
// usado em `claude.rs`, que filtra o timestamp bruto com `starts_with`.
//
// `{expr}` é o campo de timestamp em milissegundos (epoch) de cada
// tabela — muda conforme a query (`message` usa `json_extract`,
// `session` tem coluna própria).
fn filtro_periodo_sql(expr: &str) -> String {
    // `format!` com captura implícita de variável: `{expr}` dentro da string
    // é substituído pelo valor do parâmetro `expr` (funcionalidade da edition
    // 2021+, evita escrever `{}` e passar `expr` como argumento posicional
    // separado). O resultado é um fragmento de SQL (não uma query pronta),
    // devolvido como `String` porque quem chama vai colar esse texto dentro
    // de uma query maior via `format!` também. Os `?1` continuam sendo
    // placeholders do `rusqlite` — não são substituídos aqui, só quando a
    // query completa for executada com `params![periodo]`.
    format!(
        "(?1 = '' OR
          (length(?1) = 7 AND strftime('%Y-%m', {expr} / 1000, 'unixepoch', 'localtime') = ?1) OR
          (length(?1) = 10 AND date({expr} / 1000, 'unixepoch', 'localtime') = ?1))"
    )
}

// ── carregar_resumo ─────────────────────────────────────────────────
// Soma tarefas (mensagens de assistant), tokens (input + output +
// reasoning + cache read + cache write) e custo da tabela `message`.
//
// Nota: os dados ficam no campo `data` como JSON (coluna TEXT do
// SQLite). Usamos `json_extract()` da extensão JSON1 (incluída no
// `bundled` do rusqlite) para acessar os campos sem precisar de
// `serde_json` do lado Rust.
fn carregar_resumo(conn: &Connection, periodo: &str) -> rusqlite::Result<Resumo> {
    // `expr` é o SQL que extrai o timestamp de criação da mensagem (em ms)
    // de dentro do JSON armazenado na coluna `data`; é reaproveitado tanto na
    // query principal (`SUM`/`COUNT`) quanto dentro de `filtro_periodo_sql`.
    let expr = "CAST(json_extract(data, '$.time.created') AS INTEGER)";
    // `Connection::query_row` executa a query e espera EXATAMENTE uma linha
    // de resultado (erro se vier 0 ou mais de 1) — adequado aqui porque
    // `COUNT`/`SUM` sem `GROUP BY` sempre devolvem uma única linha agregada.
    // O terceiro argumento é uma closure que recebe a `Row` e monta o valor
    // de retorno; o `?` dentro dela propaga erro de tipo/coluna ausente.
    conn.query_row(
        &format!(
            "SELECT
                COUNT(*) AS tarefas,
                COALESCE(SUM(
                    COALESCE(CAST(json_extract(data, '$.tokens.input') AS INTEGER), 0) +
                    COALESCE(CAST(json_extract(data, '$.tokens.output') AS INTEGER), 0) +
                    COALESCE(CAST(json_extract(data, '$.tokens.reasoning') AS INTEGER), 0) +
                    COALESCE(CAST(json_extract(data, '$.tokens.cache.read') AS INTEGER), 0) +
                    COALESCE(CAST(json_extract(data, '$.tokens.cache.write') AS INTEGER), 0)
                ), 0) AS tokens_totais
             FROM message
             WHERE json_extract(data, '$.role') = 'assistant'
               AND {}",
            filtro_periodo_sql(expr)
        ),
        // `rusqlite::params!` monta o array de valores para os placeholders
        // `?1`, `?2`... da query; aqui só há `?1`, ligado a `periodo`.
        rusqlite::params![periodo],
        |linha| {
            Ok(Resumo {
                // `linha.get(0)` busca a coluna pelo índice posicional (0 =
                // primeira coluna do SELECT) e tenta converter para o tipo
                // inferido pelo campo da struct (`i64`); `?` propaga erro de
                // conversão sem abortar o processo.
                tarefas: linha.get(0)?,
                tokens_totais: linha.get(1)?,
            })
        },
    )
}

// ── carregar_tokens_por_dia ─────────────────────────────────────────
// Agrupa tokens por dia a partir da tabela `message`, já filtrado pelo
// período pedido. O resultado é um `BTreeMap<NaiveDate, i64>` ordenado
// por data, ideal para o heatmap (que itera em ordem cronológica).
fn carregar_tokens_por_dia(
    conn: &Connection,
    periodo: &str,
) -> rusqlite::Result<BTreeMap<NaiveDate, i64>> {
    let expr = "CAST(json_extract(data, '$.time.created') AS INTEGER)";
    // `conn.prepare` compila o SQL uma vez, devolvendo um `Statement`
    // reutilizável — diferente de `query_row`, que prepara e já executa;
    // aqui precisamos do `Statement` à parte porque vamos iterar várias
    // linhas de resultado com `query_map` (uma por dia).
    let mut stmt = conn.prepare(&format!(
        "SELECT
            date({expr} / 1000, 'unixepoch', 'localtime') AS dia,
            SUM(
                COALESCE(CAST(json_extract(data, '$.tokens.input') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.output') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.reasoning') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.cache.read') AS INTEGER), 0) +
                COALESCE(CAST(json_extract(data, '$.tokens.cache.write') AS INTEGER), 0)
            ) AS tokens
         FROM message
         WHERE json_extract(data, '$.role') = 'assistant'
           AND {}
         GROUP BY dia",
        filtro_periodo_sql(expr)
    ))?;

    // `query_map` devolve um iterador de `Result<T>` — cada linha pode
    // falhar independentemente (tipo errado, nulo inesperado). Isso
    // evita que uma linha malformada derrube o comando inteiro.
    let linhas = stmt.query_map(rusqlite::params![periodo], |linha| {
        let dia_texto: String = linha.get(0)?;
        let tokens: i64 = linha.get(1)?;
        Ok((dia_texto, tokens))
    })?;

    let mut mapa = BTreeMap::new();
    for linha in linhas {
        let (dia_texto, tokens) = linha?;
        // Data malformada (SQLite não valida formato) é descartada
        // silenciosamente — melhor que propagar erro e abortar.
        if let Ok(dia) = NaiveDate::parse_from_str(&dia_texto, "%Y-%m-%d") {
            mapa.insert(dia, tokens);
        }
    }
    Ok(mapa)
}

// ── carregar_modelos ────────────────────────────────────────────────
// Agrupa sessões por modelo + provedor (ex: "deepseek-v4-flash-free" /
// "opencode"). Retorna os dados já no formato `ModeloUso` compartilhado
// com o `claude.rs`.
// O OpenCode grava um único `cost` total por sessão (não separado por tipo
// de token), mas os tokens já vêm em colunas próprias na tabela `session`
// (`tokens_input`, `tokens_output`, `tokens_reasoning`, `tokens_cache_read`,
// `tokens_cache_write`). Usamos essas colunas para preencher os quatro
// campos de tokens do `ModeloUso` de verdade, e distribuímos o `cost` total
// entre eles proporcionalmente (`precos::distribuir_custo_proporcional`) —
// o total bate exatamente com o valor gravado pelo OpenCode, só a divisão
// entre entrada/cache/saída é uma estimativa. `tokens_reasoning` é
// contabilizado junto com a saída (é geração do modelo, não entrada).
fn carregar_modelos(conn: &Connection, periodo: &str) -> rusqlite::Result<Vec<render::ModeloUso>> {
    // Mesmo padrão de `carregar_tokens_por_dia`: prepara o `Statement` para
    // depois iterar várias linhas (uma por combinação modelo+provedor).
    let mut stmt = conn.prepare(&format!(
        "SELECT
            json_extract(model, '$.id') AS modelo,
            COALESCE(json_extract(model, '$.providerID'), 'desconhecido') AS provedor,
            COUNT(*) AS sessoes,
            COALESCE(SUM(tokens_input), 0) AS tokens_entrada,
            COALESCE(SUM(tokens_output + tokens_reasoning), 0) AS tokens_saida,
            COALESCE(SUM(tokens_cache_write), 0) AS tokens_cache_escrita,
            COALESCE(SUM(tokens_cache_read), 0) AS tokens_cache_leitura,
            COALESCE(SUM(cost), 0) AS custo_total
         FROM session
         WHERE model IS NOT NULL
           AND {}
         GROUP BY modelo, provedor
         ORDER BY sessoes DESC",
        filtro_periodo_sql("time_created")
    ))?;

    // A closure lê as colunas cruas da linha, chama a função pura de rateio
    // de custo (`distribuir_custo_proporcional`, definida em `precos.rs`) e
    // monta o `ModeloUso` já com os quatro custos calculados — assim quem
    // consome (`render.rs`) não precisa saber como o custo foi dividido.
    let linhas = stmt.query_map(rusqlite::params![periodo], |linha| {
        let tokens_entrada: i64 = linha.get(3)?;
        let tokens_saida: i64 = linha.get(4)?;
        let tokens_cache_escrita: i64 = linha.get(5)?;
        let tokens_cache_leitura: i64 = linha.get(6)?;
        let custo_total: f64 = linha.get(7)?;
        let custo = crate::ai::precos::distribuir_custo_proporcional(
            custo_total,
            tokens_entrada,
            tokens_cache_escrita,
            tokens_cache_leitura,
            tokens_saida,
        );
        Ok(render::ModeloUso {
            modelo: linha.get(0)?,
            provedor: linha.get(1)?,
            sessoes: linha.get(2)?,
            tokens_entrada,
            tokens_cache_escrita,
            tokens_cache_leitura,
            tokens_saida,
            custo_entrada: custo.entrada,
            custo_cache_escrita: custo.cache_escrita,
            custo_cache_leitura: custo.cache_leitura,
            custo_saida: custo.saida,
        })
    })?;

    // `query_map` devolve um iterador de `rusqlite::Result<ModeloUso>`;
    // `collect()` para um tipo `Result<Vec<_>, _>` faz o "curto-circuito"
    // automático: se qualquer linha vier com erro, a coleção inteira vira
    // `Err` (a inferência de tipo do retorno da função escolhe esse `collect`
    // em vez de, por exemplo, `Vec<Result<_>>`).
    linhas.collect()
}

// ── carregar_sessoes_opencode ───────────────────────────────────────
// Cada sessão do OpenCode tem várias mensagens na tabela `message`.
// Calculamos a duração como a diferença entre o primeiro e o último
// timestamp de mensagem da sessão — mesma abordagem do `claude.rs`
// para manter consistência entre os dois provedores.
//
// O resultado é `Vec<render::Sessao>`, o mesmo tipo que o claude
// produz, permitindo que `renderizar_dashboard` trate ambos. O filtro de
// período é aplicado por mensagem (antes de agrupar por sessão), então
// uma sessão que atravessa a meia-noite só conta as mensagens do dia
// pedido quando o filtro é por dia exato.
fn carregar_sessoes_opencode(
    conn: &Connection,
    periodo: &str,
) -> rusqlite::Result<Vec<render::Sessao>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT
            MIN(m.time_created) AS primeiro,
            MAX(m.time_created) AS ultimo,
            date(MIN(m.time_created) / 1000, 'unixepoch', 'localtime') AS dia
         FROM message m
         JOIN session s ON s.id = m.session_id
         WHERE s.model IS NOT NULL
           AND {}
         GROUP BY m.session_id",
        filtro_periodo_sql("m.time_created")
    ))?;

    let linhas = stmt.query_map(rusqlite::params![periodo], |linha| {
        let primeiro: i64 = linha.get(0)?;
        let ultimo: i64 = linha.get(1)?;
        let dia_texto: String = linha.get(2)?;
        Ok((primeiro, ultimo, dia_texto))
    })?;

    let mut sessoes = Vec::new();
    for linha in linhas {
        let (primeiro, ultimo, dia_texto) = linha?;
        // `let ... else`: se o parse falhar, o `else` roda e OBRIGA a sair do
        // escopo atual (aqui, `continue` pula pra próxima iteração); se
        // funcionar, `dia` fica disponível como `NaiveDate` normal daqui pra
        // baixo — sem precisar de `if let` aninhado nem `.unwrap()`.
        let Ok(dia) = NaiveDate::parse_from_str(&dia_texto, "%Y-%m-%d") else {
            continue; // data inválida — pula, não aborta
        };
        // Diferença em milissegundos → horas, clampada entre o piso
        // (1 minuto) e o teto (4 horas) definidos em `render.rs`.
        // Uma sessão que ficou aberta a noite toda não deve contar
        // como 8h de trabalho contínuo.
        let duracao = ((ultimo - primeiro) as f64 / 3600.0 / 1000.0)
            .clamp(render::MINIMO_HORAS, render::TETO_HORAS);
        sessoes.push(render::Sessao {
            dia,
            duracao_horas: duracao,
        });
    }
    Ok(sessoes)
}

// ── agregar ─────────────────────────────────────────────────────────
// Carrega tokens/dia, modelos (com estimativa de custo para `-free`) e
// sessões a partir de uma conexão já aberta, devolvendo o pacote
// compartilhado `DadosProvedor`. Privada porque só tem sentido com uma
// `Connection` já validada — quem chama de fora usa `carregar_dados`
// (caminho padrão) ou, no caso do subcomando `opencode` com `--db`
// customizado, abre a conexão em `execute()` e chama esta função direto.
fn agregar(conn: &Connection, periodo: &str) -> rusqlite::Result<render::DadosProvedor> {
    let tokens_por_dia = carregar_tokens_por_dia(conn, periodo)?;
    let mut modelos = carregar_modelos(conn, periodo)?;
    let sessoes = carregar_sessoes_opencode(conn, periodo)?;

    // Estimativa de custo para modelos `-free`: usam a taxa efetiva do
    // modelo não-free equivalente presente no próprio banco (taxa =
    // custo_real / tokens), redistribuída entre entrada/cache/saída na
    // mesma proporção do modelo free. Sem equivalente não-free no
    // período, o modelo free fica sem estimativa (entra em `sem_preco`).
    let mut modelos_sem_preco: Vec<String> = Vec::new();
    // Constrói um índice modelo-pago → (custo, tokens) para consulta O(log n)
    // depois. `iter()` empresta `modelos` (não o consome, pois ele é reusado
    // logo abaixo); `filter` descarta os `-free`; `map` extrai só o que
    // interessa; `collect()` para `BTreeMap` monta o mapa a partir do
    // iterador de tuplas `(chave, valor)`.
    let nao_free: BTreeMap<String, (f64, i64)> = modelos
        .iter()
        .filter(|m| !m.modelo.ends_with("-free"))
        .map(|m| (m.modelo.clone(), (m.custo_total(), m.tokens_totais())))
        .collect();
    // `&mut modelos`: empréstimo mutável do Vec inteiro, pois o `for` precisa
    // alterar os campos de custo de cada `ModeloUso` no lugar (em vez de
    // reconstruir o Vec). Só é possível porque `nao_free` já foi totalmente
    // calculado antes — não há conflito de empréstimos simultâneos.
    for m in &mut modelos {
        if m.modelo.ends_with("-free") {
            let base = m.modelo.trim_end_matches("-free");
            // "Let chain" (edition 2024): as duas condições (`Some` E
            // `tokens_pagos > 0`) precisam ser verdadeiras para entrar no
            // bloco, sem exigir `if let` aninhado com outro `if` dentro.
            if let Some(&(custo_pago, tokens_pagos)) = nao_free.get(base)
                && tokens_pagos > 0
            {
                let custo_estimado = m.tokens_totais() as f64 * custo_pago / tokens_pagos as f64;
                let custo = crate::ai::precos::distribuir_custo_proporcional(
                    custo_estimado,
                    m.tokens_entrada,
                    m.tokens_cache_escrita,
                    m.tokens_cache_leitura,
                    m.tokens_saida,
                );
                m.custo_entrada = custo.entrada;
                m.custo_cache_escrita = custo.cache_escrita;
                m.custo_cache_leitura = custo.cache_leitura;
                m.custo_saida = custo.saida;
            } else {
                modelos_sem_preco.push(m.modelo.clone());
            }
        }
    }

    let custo_total: f64 = modelos.iter().map(|m| m.custo_total()).sum();

    Ok(render::DadosProvedor {
        sessoes,
        modelos,
        tokens_por_dia,
        custo_total,
        sem_preco: modelos_sem_preco,
    })
}

// ── carregar_dados ──────────────────────────────────────────────────
// Wrapper público usado pelo dashboard combinado (`stats.rs`): abre o
// banco no caminho padrão e delega para `agregar`. O subcomando
// `opencode` (que aceita `--db` customizado) não passa por aqui — abre
// sua própria conexão em `execute()` e chama `agregar` direto.
pub fn carregar_dados(periodo: &str) -> Result<render::DadosProvedor, Box<dyn std::error::Error>> {
    let caminho_db = caminho_padrao_db();
    if !caminho_db.exists() {
        // `format!(...).into()`: transforma a `String` do erro no tipo de
        // retorno `Box<dyn std::error::Error>` — `String` implementa `Error`,
        // e `Box<dyn Error>` tem `From<String>`, então `.into()` encontra
        // essa conversão. É o jeito mais simples de devolver um erro "ad hoc"
        // sem precisar declarar um tipo de erro próprio.
        return Err(format!(
            "banco do OpenCode não encontrado: '{}'",
            caminho_db.display()
        )
        .into());
    }
    // `?` aqui converte o erro do `rusqlite` (`rusqlite::Error`) para
    // `Box<dyn Error>` automaticamente, via a mesma conversão `From`.
    let conn = Connection::open(&caminho_db)?;
    Ok(agregar(&conn, periodo)?)
}

// ── execute() ───────────────────────────────────────────────────────
// Método principal chamado pelo dispatch em `stats.rs`. Fluxo:
//   1. Descobre/abre o banco SQLite
//   2. Carrega o resumo e delega a agregação para `agregar`
//   3. Filtra por mês se o usuário pediu
//   4. Se `--json`, monta e retorna o JSON
//   5. Senão, delega para `render::renderizar_dashboard`
impl OpencodeArgs {
    // Assinatura padrão de todo subcomando neste projeto: `&self` (só lemos
    // os argumentos já parseados pelo clap) e `Result<String, Box<dyn Error>>`
    // — a `String` de sucesso é o texto pronto pra imprimir (`main.rs` faz
    // isso), e `Box<dyn Error>` permite propagar, com `?`, erros de origens
    // bem diferentes (SQLite, IO, JSON) sem declarar um enum de erro próprio.
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // ── Conexão com o banco ──────────────────────────────────────
        // Usa o caminho customizado (`--db`) ou o padrão
        // (`~/.local/share/opencode/opencode.db`).
        // `self.db.clone()`: `db` é `Option<PathBuf>`; clonamos porque
        // `unwrap_or_else` precisa tomar posse do valor (e `self` é só
        // emprestado). Se for `None`, chama `caminho_padrao_db` (passada por
        // nome de função, sem parênteses — vira a closure do fallback).
        let caminho_db = self.db.clone().unwrap_or_else(caminho_padrao_db);
        if !caminho_db.exists() {
            return Err(format!(
                "banco do OpenCode não encontrado: '{}'",
                caminho_db.display()
            )
            .into());
        }

        let conn = Connection::open(&caminho_db)?;

        // ── Define o filtro de período ────────────────────────────────
        let periodo = if self.historico {
            String::new()
        } else {
            self.periodo
                .clone()
                .unwrap_or_else(|| Local::now().format("%Y-%m").to_string())
        };

        // `resumo` (tarefas/tokens brutos da tabela `message`) só é usado
        // pelo subtítulo/JSON deste comando — não faz parte do pacote
        // compartilhado `DadosProvedor`, que vem de `agregar`.
        let resumo = carregar_resumo(&conn, &periodo)?;
        let dados = agregar(&conn, &periodo)?;

        // ── Subtítulo do dashboard ───────────────────────────────────
        let subtitulo = if self.historico {
            format!(
                "{} tokens totais ({} tarefas)",
                render::numero_compacto(resumo.tokens_totais as f64),
                resumo.tarefas
            )
        } else {
            periodo.clone()
        };

        // ── JSON (early return) ──────────────────────────────────────
        if self.json {
            // Structs declaradas DENTRO da função: só existem no escopo deste
            // `if`, porque só servem para dar forma ao JSON de saída deste
            // comando — não fazem sentido em outro lugar do módulo. Cada uma
            // tem `#[derive(serde::Serialize)]`, que gera o código que
            // converte a struct para JSON (o `serde` olha o nome e tipo de
            // cada campo e escreve o objeto correspondente); é por isso que
            // os nomes dos campos aqui viram exatamente as chaves do JSON.
            #[derive(serde::Serialize)]
            struct LinhaDiaTokens {
                dia: String,
                tokens: i64,
            }
            #[derive(serde::Serialize)]
            struct LinhaDiaSessao {
                dia: String,
                horas: f64,
                sessoes: u32,
            }
            #[derive(serde::Serialize)]
            struct SaidaSessao {
                total_horas: f64,
                sessoes: usize,
                dias: Vec<LinhaDiaSessao>,
            }
            // Struct "raiz" do JSON: agrupa tudo que o comando expõe quando
            // chamado com `--json`, espelhando (em formato de dados) o mesmo
            // conteúdo que o dashboard colorido mostra em texto.
            #[derive(serde::Serialize)]
            struct Saida {
                historico: bool,
                mes: String,
                resumo_tarefas: i64,
                tokens_totais: i64,
                custo_usd: f64,
                modelos_sem_preco: Vec<String>,
                tokens_por_dia: Vec<LinhaDiaTokens>,
                modelos: Vec<render::ModeloUso>,
                sessoes_horas: SaidaSessao,
            }
            let por_dia_horas = render::agregar_por_dia(&dados.sessoes);
            // `.values()` itera só os valores do mapa (ignora as chaves/dias);
            // `map(|(h, _)| h)` desestrutura a tupla `(horas, sessoes)` e
            // descarta a segunda parte; `.sum()` reduz tudo a um único `f64`.
            let total_horas: f64 = por_dia_horas.values().map(|(h, _)| h).sum();
            let saida_json = Saida {
                historico: self.historico,
                mes: subtitulo.clone(),
                resumo_tarefas: resumo.tarefas,
                tokens_totais: resumo.tokens_totais,
                custo_usd: dados.custo_total,
                modelos_sem_preco: dados.sem_preco,
                tokens_por_dia: dados
                    .tokens_por_dia
                    .iter()
                    .map(|(dia, tokens)| LinhaDiaTokens {
                        dia: dia.to_string(),
                        tokens: *tokens,
                    })
                    .collect(),
                modelos: dados.modelos,
                sessoes_horas: SaidaSessao {
                    total_horas,
                    sessoes: dados.sessoes.len(),
                    dias: por_dia_horas
                        .iter()
                        .map(|(dia, (horas, sessoes))| LinhaDiaSessao {
                            dia: dia.to_string(),
                            horas: *horas,
                            sessoes: *sessoes,
                        })
                        .collect(),
                },
            };
            // `to_string_pretty` serializa a struct para uma `String` JSON
            // indentada (legível para humanos, ao contrário de `to_string`,
            // que gera JSON compacto numa linha só); o `?` propaga o erro se
            // algum campo não puder ser serializado (não deveria acontecer
            // aqui, já que todos os tipos envolvidos implementam `Serialize`).
            return Ok(serde_json::to_string_pretty(&saida_json)?);
        }

        // ── Dashboard visual (delega para render.rs) ─────────────────
        Ok(render::renderizar_dashboard(
            "OpenCode atividade",
            &subtitulo,
            &dados.tokens_por_dia,
            &dados.sessoes,
            &dados.modelos,
            dados.custo_total,
            &dados.sem_preco,
            self.weeks,
            !self.no_color,
            None, // sem top dias
        ))
    }
}
