use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use chrono::{DateTime, Local, NaiveDate, Utc};
use serde::Deserialize;
use walkdir::WalkDir;

use crate::horas_sessao;

// Custos de IA do Claude Code — parte pura, compartilhada pelo CLI
// (`dev-cli ai stats claude`) e pelo dev-server (`/api/ia/custos`).
// Mudou de morada: era `crates/cli/src/ai/precos.rs`; veio para o nucleo
// porque o servidor não pode depender do crate do CLI (a convenção do
// workspace: cálculo puro no nucleo, casca de apresentação nos bins).

// Tabela de preços por modelo (USD por milhão de tokens). Preços refletem
// as taxas publicadas pela Anthropic em 2026-06; como a Anthropic muda
// preços e lança modelos novos, esta tabela precisa de manutenção manual
// (ver skill `claude-api` do harness pra conferir os valores atuais antes de
// confiar cegamente num número antigo).

/// Preço de um modelo em dólares por milhão de tokens ("mtok" = 1_000_000
/// tokens), separado por direção do tráfego: `entrada_por_mtok` é o que a
/// Anthropic cobra pelos tokens que enviamos (prompt), `saida_por_mtok` pelos
/// tokens que o modelo gera (resposta) — sempre mais caro, porque gerar texto
/// é mais custoso computacionalmente do que processar o que já foi lido.
pub struct Preco {
    pub entrada_por_mtok: f64,
    pub saida_por_mtok: f64,
}

// Tabela de preços por nome de modelo. `&str` como chave do `match` compara
// o texto exatamente; várias variantes (`"claude-opus-4-8" | "claude-opus-4-7" | ...`)
// caem no mesmo braço porque tiveram o mesmo preço nas últimas revisões da
// Anthropic. Devolve `Option<Preco>`: `None` quando o modelo não está
// cadastrado aqui (modelo novo, nome digitado diferente, etc.) — quem chama
// decide o que fazer na ausência (normalmente exibir "não estimado").
// docs: https://doc.rust-lang.org/std/option/enum.Option.html
// docs: https://doc.rust-lang.org/std/option/enum.Option.html#variant.None
pub fn preco_do_modelo(modelo: &str) -> Option<Preco> {
    match modelo {
        "claude-opus-4-8" | "claude-opus-4-7" | "claude-opus-4-6" => Some(Preco {
            entrada_por_mtok: 5.0,
            saida_por_mtok: 25.0,
        }),
        "claude-sonnet-5" | "claude-sonnet-4-6" | "claude-sonnet-4-5" => Some(Preco {
            entrada_por_mtok: 3.0,
            saida_por_mtok: 15.0,
        }),
        "claude-haiku-4-5" | "claude-haiku-4-5-20251001" => Some(Preco {
            entrada_por_mtok: 1.0,
            saida_por_mtok: 5.0,
        }),
        "claude-fable-5" => Some(Preco {
            entrada_por_mtok: 10.0,
            saida_por_mtok: 50.0,
        }),
        _ => None,
    }
}

// Cache write e cache read não têm preço próprio por modelo — a Anthropic
// cobra os dois como múltiplos do preço de entrada "fresca" desse modelo,
// numa proporção parecida entre os modelos. Escrita custa mais (o modelo
// precisa processar e gravar o cache); leitura custa bem menos (só
// reaproveita o que já foi processado). Aproximação: usamos a taxa de TTL
// de 5 minutos pra escrita (a mais comum) em vez da de 1 hora (que é 2x);
// os transcritos não distinguem qual TTL foi usado em cada mensagem.
//
// Não existem outros tipos de cobrança de token além destes quatro (entrada,
// cache write, cache read, saída) — não há taxa separada para thinking,
// tool use, etc.; tudo isso já é contado dentro de entrada/saída pelo
// próprio `usage` da API.
const CACHE_ESCRITA_MULTIPLICADOR: f64 = 1.25;
const CACHE_LEITURA_MULTIPLICADOR: f64 = 0.1;

// Proporção usada só como estimativa quando não temos o custo já separado
// por tipo de token (ex: OpenCode, que grava no banco um único `cost` total
// por sessão). A razão saída:entrada = 5:1 é constante entre os modelos da
// Anthropic (Opus 5/25, Sonnet 3/15, Haiku 1/5, Fable 10/50) — usamos o
// mesmo fator aqui como aproximação razoável para modelos de outros
// provedores, cuja tabela de preços real não temos.
const RAZAO_SAIDA_ENTRADA: f64 = 5.0;

/// Custo em USD já separado pelos quatro tipos de cobrança de token:
/// entrada "fresca", cache write, cache read e saída. `total()` soma os
/// quatro para o custo completo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CustoDetalhado {
    pub entrada: f64,
    pub cache_escrita: f64,
    pub cache_leitura: f64,
    pub saida: f64,
}

impl CustoDetalhado {
    /// Soma os quatro componentes de custo num único valor em USD.
    /// `&self`: só lemos os campos, não precisamos consumir a struct.
    pub fn total(&self) -> f64 {
        self.entrada + self.cache_escrita + self.cache_leitura + self.saida
    }
}

// Núcleo puro: tokens -> custo em USD por tipo, ou `None` se o modelo não
// estiver na tabela (o relatório mostra "não estimado" em vez de inventar
// um número).
// docs: https://doc.rust-lang.org/std/option/enum.Option.html#variant.None
pub fn calcular_custo_detalhado(
    modelo: &str,
    tokens_entrada: i64,
    tokens_cache_escrita: i64,
    tokens_cache_leitura: i64,
    tokens_saida: i64,
) -> Option<CustoDetalhado> {
    // `?` no `Option`: se `preco_do_modelo` devolver `None` (modelo
    // desconhecido), a função inteira retorna `None` aqui mesmo, sem
    // precisarmos escrever um `match`/`if let` explícito.
    // docs: https://doc.rust-lang.org/std/option/index.html#the-question-mark-operator-
    // docs: https://doc.rust-lang.org/std/option/enum.Option.html#variant.None
    let preco = preco_do_modelo(modelo)?;
    // Recebe a taxa como parâmetro em vez de fixá-la: assim o mesmo cálculo
    // (tokens / 1M * taxa) serve pras quatro taxas diferentes abaixo, sem
    // repetir a fórmula quatro vezes. `|tokens: i64, taxa_por_mtok: f64| ...`
    // é uma closure (função anônima) que captura nada do ambiente externo —
    // só recebe os dois parâmetros e devolve o custo em dólares.
    let custo = |tokens: i64, taxa_por_mtok: f64| tokens as f64 / 1_000_000.0 * taxa_por_mtok;
    // `tokens as f64`: conversão explícita de inteiro (`i64`, contagem exata
    // de tokens) para ponto flutuante (`f64`), necessária porque o resultado
    // da divisão por 1_000_000.0 é fracionário (ex.: 0.5 milhão de tokens).
    // `Some(...)`: envolve o resultado no variante presente do `Option`,
    // já que a função só chega até aqui quando o modelo foi encontrado.
    // docs: https://doc.rust-lang.org/std/primitive.i64.html
    // docs: https://doc.rust-lang.org/std/primitive.f64.html
    // docs: https://doc.rust-lang.org/std/option/enum.Option.html#variant.Some
    // docs: https://doc.rust-lang.org/std/option/enum.Option.html
    Some(CustoDetalhado {
        entrada: custo(tokens_entrada, preco.entrada_por_mtok),
        // Cache write usa a MESMA taxa de entrada do modelo, só que
        // multiplicada pelo fator de 1.25 (25% mais caro que entrada fresca).
        cache_escrita: custo(
            tokens_cache_escrita,
            preco.entrada_por_mtok * CACHE_ESCRITA_MULTIPLICADOR,
        ),
        // Cache read usa a taxa de entrada multiplicada por 0.1 (90% mais
        // barato que entrada fresca).
        cache_leitura: custo(
            tokens_cache_leitura,
            preco.entrada_por_mtok * CACHE_LEITURA_MULTIPLICADOR,
        ),
        saida: custo(tokens_saida, preco.saida_por_mtok),
    })
}

/// Distribui um custo total já conhecido (ex: valor gravado pelo OpenCode,
/// que não separa por tipo de token) entre os quatro tipos de cobrança,
/// proporcionalmente ao "peso" de cada tipo de token — peso esse calculado
/// com os mesmos multiplicadores de cache e a razão saída:entrada usados
/// pela Anthropic. É uma estimativa: o total bate exatamente com
/// `custo_total`, mas a divisão entre entrada/cache/saída é aproximada.
pub fn distribuir_custo_proporcional(
    custo_total: f64,
    tokens_entrada: i64,
    tokens_cache_escrita: i64,
    tokens_cache_leitura: i64,
    tokens_saida: i64,
) -> CustoDetalhado {
    // "Peso" aqui não é dólar, é uma unidade comum que reflete quanto cada
    // tipo de token pesa relativo à entrada fresca (peso 1.0 por token de
    // entrada). Cache write pesa mais (1.25x), cache read pesa menos (0.1x),
    // e saída pesa `RAZAO_SAIDA_ENTRADA` (5x) porque é sempre mais cara.
    // Multiplicar a contagem de tokens pelo peso dá o "tamanho" relativo de
    // cada fatia do custo total.
    let peso_entrada = tokens_entrada as f64;
    let peso_cache_escrita = tokens_cache_escrita as f64 * CACHE_ESCRITA_MULTIPLICADOR;
    let peso_cache_leitura = tokens_cache_leitura as f64 * CACHE_LEITURA_MULTIPLICADOR;
    let peso_saida = tokens_saida as f64 * RAZAO_SAIDA_ENTRADA;
    let peso_total = peso_entrada + peso_cache_escrita + peso_cache_leitura + peso_saida;

    // Guarda contra divisão por zero: sem nenhum token registrado, não há
    // como saber a proporção — devolvemos tudo zerado em vez de um `NaN`
    // (resultado de `0.0 / 0.0` em ponto flutuante).
    if peso_total <= 0.0 {
        return CustoDetalhado {
            entrada: 0.0,
            cache_escrita: 0.0,
            cache_leitura: 0.0,
            saida: 0.0,
        };
    }

    // `fator` converte peso em dólares: é quanto vale "1 unidade de peso"
    // para que a soma das quatro fatias bata exatamente com `custo_total`.
    let fator = custo_total / peso_total;
    CustoDetalhado {
        entrada: peso_entrada * fator,
        cache_escrita: peso_cache_escrita * fator,
        cache_leitura: peso_cache_leitura * fator,
        saida: peso_saida * fator,
    }
}

// ── UsoSessao ───────────────────────────────────────────────────────
// Dados de uso de cada mensagem de assistente, extraídos dos JSONL.
// Usado para agregar custo e tokens por modelo no servidor.
pub struct UsoSessao {
    pub modelo: String,
    pub tokens_entrada: i64,
    pub tokens_cache_escrita: i64,
    pub tokens_cache_leitura: i64,
    pub tokens_saida: i64,
}

// ── Structs de deserialização dos JSONL ────────────────────────────
// Cada linha do JSONL do Claude tem esta estrutura. Só declaramos os
// campos que nos interessam; extras são ignorados pelo serde.

#[derive(Debug, Deserialize)]
struct Uso {
    input_tokens: i64,
    output_tokens: i64,
    #[serde(default)]
    cache_creation_input_tokens: i64,
    #[serde(default)]
    cache_read_input_tokens: i64,
}

#[derive(Debug, Deserialize)]
struct Mensagem {
    model: Option<String>,
    usage: Option<Uso>,
}

#[derive(Debug, Deserialize)]
struct Registro {
    timestamp: String,
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    message: Option<Mensagem>,
}

// ── carregar_sessoes ──────────────────────────────────────────────
// Lê todos os `.jsonl` sob `dir`, filtra pelo `mes` pedido (ex: "2026-06")
// e devolve três estruturas:
//
//   (a) `Vec<Sessao>` — uma por sessão, com data e duração em horas
//       (primeiro→último timestamp, clampado).
//   (b) `Vec<UsoSessao>` — uma por mensagem de assistente, com modelo
//       e tokens — usada para calcular custo e agregar por modelo.
//   (c) `BTreeMap<NaiveDate, i64>` — tokens agregados por dia, usado
//       para o heatmap.
//
// `dir` é um parâmetro (não lê env) — o CLI resolve `~/.claude/projects`,
// o servidor usa `DEV_CLI_CLAUDE_PROJETOS_DIR`; a função pura só itera.
// WalkDir itera recursivamente sem precisarmos escrever a recursão manual.
// Arquivos ilegíveis ou linhas malformadas são puladas silenciosamente.
pub fn carregar_sessoes(
    dir: &Path,
    mes: &str,
) -> (
    Vec<horas_sessao::Sessao>,
    Vec<UsoSessao>,
    BTreeMap<NaiveDate, i64>,
) {
    let mut horarios_por_sessao: HashMap<String, Vec<DateTime<Utc>>> = HashMap::new();
    let mut usos: Vec<UsoSessao> = Vec::new();
    let mut tokens_por_dia: BTreeMap<NaiveDate, i64> = BTreeMap::new();

    let arquivos = WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entrada| entrada.path().extension().is_some_and(|ext| ext == "jsonl"));

    for entrada in arquivos {
        let Ok(conteudo) = std::fs::read_to_string(entrada.path()) else {
            continue;
        };

        for linha in conteudo.lines() {
            let Ok(registro) = serde_json::from_str::<Registro>(linha) else {
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

            if let Some(ref mensagem) = registro.message
                && let Some(ref uso) = mensagem.usage
            {
                let total = uso.input_tokens
                    + uso.output_tokens
                    + uso.cache_creation_input_tokens
                    + uso.cache_read_input_tokens;
                let dia = instante.with_timezone(&Local).date_naive();
                *tokens_por_dia.entry(dia).or_insert(0) += total;

                usos.push(UsoSessao {
                    modelo: mensagem
                        .model
                        .clone()
                        .unwrap_or_else(|| "desconhecido".to_string()),
                    tokens_entrada: uso.input_tokens,
                    tokens_cache_escrita: uso.cache_creation_input_tokens,
                    tokens_cache_leitura: uso.cache_read_input_tokens,
                    tokens_saida: uso.output_tokens,
                });
            }
        }
    }

    let sessoes = horarios_por_sessao
        .into_values()
        .filter_map(|mut horarios| {
            horarios.sort();
            let duracao_horas = horas_sessao::duracao_sessao(&horarios)?;
            let dia = horarios.first()?.with_timezone(&Local).date_naive();
            Some(horas_sessao::Sessao { dia, duracao_horas })
        })
        .collect();

    (sessoes, usos, tokens_por_dia)
}

// `#[cfg(test)]`: este módulo só entra na compilação quando rodamos
// `cargo test` — no binário final ele nem existe. `use super::*` traz tudo
// do módulo pai (funções, structs e constantes) para o escopo dos testes.
// docs: https://doc.rust-lang.org/reference/conditional-compilation.html#the-cfg-attribute
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calcula_custo_para_modelo_conhecido() {
        let custo = calcular_custo_detalhado("claude-sonnet-5", 1_000_000, 0, 0, 1_000_000)
            .expect("modelo conhecido deve ter preço");
        assert_eq!(custo.total(), 18.0); // $3 entrada + $15 saída, por MTok, sem cache
    }

    #[test]
    fn cache_escrita_custa_mais_que_entrada_fresca() {
        let custo = calcular_custo_detalhado("claude-sonnet-5", 0, 1_000_000, 0, 0)
            .expect("modelo conhecido deve ter preço");
        assert_eq!(custo.cache_escrita, 3.75); // $3 * 1.25 por MTok de cache write
    }

    #[test]
    fn cache_leitura_custa_bem_menos_que_entrada_fresca() {
        let custo = calcular_custo_detalhado("claude-sonnet-5", 0, 0, 1_000_000, 0)
            .expect("modelo conhecido deve ter preço");
        // $3 * 0.1 por MTok de cache read — compara com tolerância porque
        // 3.0 * 0.1 não é representável exatamente em ponto flutuante
        // (dá 0.30000000000000004), então `assert_eq!` direto quebraria.
        // docs: https://doc.rust-lang.org/std/macro.assert_eq.html
        let diferenca = (custo.cache_leitura - 0.3).abs();
        assert!(diferenca < 1e-9, "custo {custo:?} deveria ser ~0.3");
    }

    #[test]
    fn modelo_desconhecido_nao_estima_custo() {
        assert_eq!(
            calcular_custo_detalhado("modelo-inexistente", 100, 0, 0, 100),
            None
        );
    }

    #[test]
    fn calcula_custo_cache_separadamente_da_entrada_e_saida() {
        let custo = calcular_custo_detalhado("claude-sonnet-5", 0, 1_000_000, 0, 0)
            .expect("modelo conhecido deve ter preço");
        assert_eq!(custo.entrada, 0.0);
        assert_eq!(custo.saida, 0.0);
        assert_eq!(custo.cache_escrita, 3.75); // $3 * 1.25 por MTok
    }

    #[test]
    fn calcula_custo_para_opus_com_preco_atualizado() {
        let custo = calcular_custo_detalhado("claude-opus-4-8", 1_000_000, 0, 0, 1_000_000)
            .expect("modelo conhecido deve ter preço");
        assert_eq!(custo.total(), 30.0); // $5 entrada + $25 saída, por MTok, sem cache
    }

    #[test]
    fn distribui_custo_proporcional_preserva_o_total() {
        let detalhado = distribuir_custo_proporcional(12.0, 1_000_000, 500_000, 2_000_000, 300_000);
        let diferenca = (detalhado.total() - 12.0).abs();
        assert!(diferenca < 1e-9, "total {detalhado:?} deveria somar 12.0");
    }

    #[test]
    fn distribui_custo_proporcional_sem_tokens_e_tudo_zero() {
        let detalhado = distribuir_custo_proporcional(5.0, 0, 0, 0, 0);
        assert_eq!(
            detalhado,
            CustoDetalhado {
                entrada: 0.0,
                cache_escrita: 0.0,
                cache_leitura: 0.0,
                saida: 0.0,
            }
        );
    }

    #[test]
    fn carregar_sessoes_le_jsonl_filtra_mes_e_agrega_tokens() {
        let dir = tempfile::tempdir().unwrap();
        // Duas mensagens de julho (mesma sessão) + uma de junho (fora do mês).
        let jsonl = concat!(
            r#"{"timestamp":"2026-07-10T10:00:00-03:00","sessionId":"s1","message":{"model":"claude-sonnet-5","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":10,"cache_read_input_tokens":1000}}}"#,
            "\n",
            r#"{"timestamp":"2026-07-10T10:30:00-03:00","sessionId":"s1","message":{"model":"claude-sonnet-5","usage":{"input_tokens":200,"output_tokens":80,"cache_creation_input_tokens":0,"cache_read_input_tokens":2000}}}"#,
            "\n",
            r#"{"timestamp":"2026-06-01T09:00:00-03:00","sessionId":"s0","message":{"model":"claude-sonnet-5","usage":{"input_tokens":999,"output_tokens":9,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#,
            "\n",
            "linha invalida que o parser deve pular\n",
        );
        std::fs::create_dir(dir.path().join("projeto-a")).unwrap();
        std::fs::write(dir.path().join("projeto-a/sessao.jsonl"), jsonl).unwrap();

        let (sessoes, usos, tokens_por_dia) = carregar_sessoes(dir.path(), "2026-07");

        assert_eq!(sessoes.len(), 1, "uma sessão em julho");
        assert_eq!(usos.len(), 2, "duas mensagens de assistente em julho");
        let dia = chrono::NaiveDate::from_ymd_opt(2026, 7, 10).unwrap();
        // 100+50+10+1000 + 200+80+0+2000 = 3440 tokens no dia 10/07.
        assert_eq!(tokens_por_dia.get(&dia), Some(&3440));
        assert_eq!(usos[0].modelo, "claude-sonnet-5");
    }
}
