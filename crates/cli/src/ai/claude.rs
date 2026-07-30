// Subcomando `ai stats claude`: horas trabalhadas + custo estimado + heatmap
// a partir dos transcritos locais do Claude Code (~/.claude/projects/**/*.jsonl).
//
// Estrutura idêntica à do `opencode.rs`: carrega dados (IO) e delega a
// renderização do dashboard para `render::renderizar_dashboard`. A única
// diferença é a fonte — arquivos JSONL em vez de SQLite.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{Local, NaiveDate};
use clap::Args;

use crate::ai::render;

/// Estatísticas do Claude Code a partir dos transcritos JSONL locais.
// `#[derive(Args, Debug)]`: `Args` faz o clap gerar o parser deste grupo de
// argumentos a partir dos campos abaixo; `Debug` permite imprimir a struct
// inteira com `{:?}` (útil ao depurar quais flags foram recebidas).
// `#[command(help_template = ...)]`: troca o texto de ajuda padrão do clap
// pelo template compartilhado do módulo `crate::help`.
// docs: https://docs.rs/clap/latest/clap/trait.Args.html
// docs: https://doc.rust-lang.org/std/fmt/trait.Debug.html
#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct ClaudeArgs {
    /// Período: mês (YYYY-MM) ou dia (YYYY-MM-DD). Se omitido, usa o mês atual.
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    periodo: Option<String>,

    /// Mostra todo o histórico (ignora o filtro de período).
    #[arg(long, conflicts_with = "periodo", help_heading = crate::help::OPCOES)]
    historico: bool,

    /// Largura do heatmap em semanas (4-104, padrão 52).
    #[arg(long, default_value_t = 52, value_parser = clap::value_parser!(u32).range(4..=104), help_heading = crate::help::OPCOES)]
    weeks: u32,

    /// Quantos dias mostrar no ranking dos mais intensos (padrão 5).
    #[arg(long, short, default_value_t = 5, help_heading = crate::help::OPCOES)]
    top: usize,

    /// Desativa cores ANSI (útil para pipes/arquivos).
    #[arg(long, help_heading = crate::help::OPCOES)]
    no_color: bool,

    /// Em vez do dashboard, imprime JSON com os dados brutos.
    #[arg(long, help_heading = crate::help::OPCOES)]
    json: bool,
}

// Re-export do nucleo: UsoSessao e carregar_sessoes agora vivem em
// `nucleo::ia_claude` para serem compartilhados com o dev-server —
// o CLI mantém um wrapper que injeta `diretorio_projetos()` como `dir`.
pub use nucleo::ia_claude::UsoSessao;

// ── Helpers ─────────────────────────────────────────────────────────
// Diretório onde o Claude Code salva os transcritos das sessões. Cada
// projeto tem uma subpasta com arquivos `.jsonl`.
fn diretorio_projetos() -> PathBuf {
    // `std::env::var` devolve `Result<String, VarError>`: `Err` se a
    // variável não existir. `unwrap_or_else` cai para "." (diretório atual)
    // nesse caso, em vez de entrar em pânico — mantendo a função livre de
    // `unwrap()` conforme convenção do projeto.
    // docs: https://doc.rust-lang.org/std/env/fn.var.html
    // docs: https://doc.rust-lang.org/std/env/enum.VarError.html
    // docs: https://doc.rust-lang.org/std/result/enum.Result.html
    // docs: https://doc.rust-lang.org/std/result/enum.Result.html#method.unwrap_or_else
    // docs: https://doc.rust-lang.org/std/result/enum.Result.html#method.unwrap
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".claude/projects")
}

// ── carregar_sessoes (wrapper) ─────────────────────────────────────
// A resolução do diretório (~/.claude/projects) é responsabilidade da
// casca; a função pura do nucleo recebe o caminho pronto — assim o
// dev-server pode apontar para outro diretório (env de teste) sem que a
// lógica de parse mude de comportamento.
fn carregar_sessoes(
    mes: &str,
) -> (
    Vec<render::Sessao>,
    Vec<UsoSessao>,
    BTreeMap<NaiveDate, i64>,
) {
    nucleo::ia_claude::carregar_sessoes(&diretorio_projetos(), mes)
}

// ── carregar_dados ──────────────────────────────────────────────────
// Carrega as sessões do período e agrega tokens/custo por modelo,
// devolvendo o pacote compartilhado `DadosProvedor`. Extraído de
// `execute()` para ser reaproveitado pelo dashboard combinado (`ai
// stats`, sem subcomando, em `stats.rs`) sem duplicar esta lógica.
pub fn carregar_dados(periodo: &str) -> render::DadosProvedor {
    let (sessoes, usos, tokens_por_dia) = carregar_sessoes(periodo);

    let mut custo_usd_total = 0.0;
    // `BTreeSet`: como um `BTreeMap` mas só guarda as chaves (sem valor
    // associado) e não permite duplicatas — perfeito para "quais nomes de
    // modelo eu já vi sem preço cadastrado", onde só a presença importa.
    // docs: https://doc.rust-lang.org/std/collections/struct.BTreeSet.html
    // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
    let mut modelos_sem_preco: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    let mut por_modelo: BTreeMap<String, render::ModeloUso> = BTreeMap::new();

    for uso in &usos {
        // "<synthetic>" é o placeholder interno do Claude Code para
        // mensagens de erro/rate-limit — não é uso real de um modelo
        // (tokens sempre zerados), então não entra na tabela.
        if uso.modelo == "<synthetic>" {
            continue;
        }

        // API `entry`: busca (ou cria, via `or_insert`) a entrada para este
        // modelo no mapa. Na primeira vez que um modelo aparece, o
        // `render::ModeloUso` literal abaixo é inserido com contadores
        // zerados; nas próximas iterações, `entry` só devolve o valor já
        // existente para acumularmos nele.
        // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.entry
        // docs: https://doc.rust-lang.org/std/collections/btree_map/enum.Entry.html#method.or_insert
        let entry = por_modelo
            .entry(uso.modelo.clone())
            .or_insert(render::ModeloUso {
                modelo: uso.modelo.clone(),
                provedor: "anthropic".to_string(),
                sessoes: 0,
                tokens_entrada: 0,
                tokens_cache_escrita: 0,
                tokens_cache_leitura: 0,
                tokens_saida: 0,
                custo_entrada: 0.0,
                custo_cache_escrita: 0.0,
                custo_cache_leitura: 0.0,
                custo_saida: 0.0,
            });
        entry.tokens_entrada += uso.tokens_entrada;
        entry.tokens_cache_escrita += uso.tokens_cache_escrita;
        entry.tokens_cache_leitura += uso.tokens_cache_leitura;
        entry.tokens_saida += uso.tokens_saida;
        entry.sessoes += 1;

        if let Some(custo) = crate::ai::precos::calcular_custo_detalhado(
            &uso.modelo,
            uso.tokens_entrada,
            uso.tokens_cache_escrita,
            uso.tokens_cache_leitura,
            uso.tokens_saida,
        ) {
            custo_usd_total += custo.total();
            entry.custo_entrada += custo.entrada;
            entry.custo_cache_escrita += custo.cache_escrita;
            entry.custo_cache_leitura += custo.cache_leitura;
            entry.custo_saida += custo.saida;
        } else {
            modelos_sem_preco.insert(uso.modelo.clone());
        }
    }

    render::DadosProvedor {
        sessoes,
        modelos: por_modelo.into_values().collect(),
        tokens_por_dia,
        custo_total: custo_usd_total,
        sem_preco: modelos_sem_preco.into_iter().collect(),
    }
}

// ── execute() ───────────────────────────────────────────────────────
// Resolve o período, carrega os dados via `carregar_dados` e delega
// para JSON ou `render::renderizar_dashboard`.
impl ClaudeArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Três casos para o período: `--historico` força string vazia (que
        // `carregar_sessoes`/`carregar_dados` tratam como "sem filtro, mostra
        // tudo"); um período explícito (`self.periodo`) é clonado e usado
        // como veio; sem nenhum dos dois, cai no mês atual formatado como
        // "YYYY-MM" (`Local::now()` pega a data local, não UTC).
        // docs: https://docs.rs/chrono/latest/chrono/offset/struct.Local.html#method.now
        let periodo = if self.historico {
            String::new()
        } else {
            self.periodo
                .clone()
                .unwrap_or_else(|| Local::now().format("%Y-%m").to_string())
        };

        let dados = carregar_dados(&periodo);
        if dados.sessoes.is_empty() && dados.tokens_por_dia.is_empty() {
            return Ok(format!("Nenhuma sessão encontrada para {periodo}"));
        }

        let por_dia = render::agregar_por_dia(&dados.sessoes);
        // `.values()` itera só os valores do mapa (descarta as chaves/dias);
        // cada valor é a tupla `(horas, sessoes)` — `map` extrai só `horas`
        // e `sum()` soma tudo num único `f64`.
        // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.values
        // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.map
        // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.sum
        let total_horas: f64 = por_dia.values().map(|(h, _)| h).sum();
        let subtitulo = if self.historico {
            format!(
                "{:.1}h totais em {} sessões",
                total_horas,
                dados.sessoes.len()
            )
        } else {
            periodo.clone()
        };

        if self.json {
            // Structs de saída declaradas aqui dentro (escopo local a este
            // `if`): só existem para dar forma ao JSON impresso neste modo,
            // não são usadas em mais nenhum lugar do módulo. `Serialize` (do
            // serde) é o inverso de `Deserialize`: converte a struct em texto
            // JSON em vez de ler JSON para dentro dela.
            // docs: https://docs.rs/serde/latest/serde/trait.Serialize.html
            // docs: https://docs.rs/serde/latest/serde/trait.Deserialize.html
            #[derive(serde::Serialize)]
            struct LinhaDia {
                dia: String,
                horas: f64,
                sessoes: u32,
            }
            #[derive(serde::Serialize)]
            struct LinhaDiaTokens {
                dia: String,
                tokens: i64,
            }
            #[derive(serde::Serialize)]
            struct Saida {
                historico: bool,
                mes: String,
                total_horas: f64,
                dias: Vec<LinhaDia>,
                custo_usd_total: f64,
                modelos: Vec<render::ModeloUso>,
                modelos_sem_preco: Vec<String>,
                tokens_por_dia: Vec<LinhaDiaTokens>,
            }
            let saida_json = Saida {
                historico: self.historico,
                mes: periodo.clone(),
                total_horas,
                // `.iter()` empresta cada par `(dia, (horas, sessoes))` do
                // `BTreeMap` sem consumi-lo; `*horas`/`*sessoes` desreferenciam
                // os valores emprestados para copiá-los (são tipos `Copy`)
                // para dentro da struct de saída.
                // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.iter
                // docs: https://doc.rust-lang.org/std/marker/trait.Copy.html
                dias: por_dia
                    .iter()
                    .map(|(dia, (horas, sessoes))| LinhaDia {
                        dia: dia.to_string(),
                        horas: *horas,
                        sessoes: *sessoes,
                    })
                    .collect(),
                custo_usd_total: dados.custo_total,
                modelos: dados.modelos,
                modelos_sem_preco: dados.sem_preco,
                tokens_por_dia: dados
                    .tokens_por_dia
                    .iter()
                    .map(|(dia, tokens)| LinhaDiaTokens {
                        dia: dia.to_string(),
                        tokens: *tokens,
                    })
                    .collect(),
            };
            // `?` propaga o erro de serialização (praticamente nunca ocorre
            // aqui, mas o tipo de retorno de `to_string_pretty` é `Result`).
            // docs: https://docs.rs/serde_json/latest/serde_json/fn.to_string_pretty.html
            // docs: https://doc.rust-lang.org/std/result/enum.Result.html
            return Ok(serde_json::to_string_pretty(&saida_json)?);
        }

        // Caminho padrão (sem `--json`): delega toda a renderização colorida
        // (heatmap, tabela de modelos, ranking de dias) para `render`,
        // compartilhada com o `opencode.rs` e o dashboard combinado.
        Ok(render::renderizar_dashboard(
            "Claude Code atividade",
            &subtitulo,
            &dados.tokens_por_dia,
            &dados.sessoes,
            &dados.modelos,
            dados.custo_total,
            &dados.sem_preco,
            self.weeks,
            !self.no_color,
            Some(self.top),
        ))
    }
}
