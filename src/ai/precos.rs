// Tabela de preços por modelo (USD por milhão de tokens). Preços refletem
// as taxas publicadas pela Anthropic em 2026-06; como a Anthropic muda
// preços e lança modelos novos, esta tabela precisa de manutenção manual
// (ver skill `claude-api` do harness pra conferir os valores atuais antes de
// confiar cegamente num número antigo).

pub struct Preco {
    pub entrada_por_mtok: f64,
    pub saida_por_mtok: f64,
}

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
    pub fn total(&self) -> f64 {
        self.entrada + self.cache_escrita + self.cache_leitura + self.saida
    }
}

// Núcleo puro: tokens -> custo em USD por tipo, ou `None` se o modelo não
// estiver na tabela (o relatório mostra "não estimado" em vez de inventar
// um número).
pub fn calcular_custo_detalhado(
    modelo: &str,
    tokens_entrada: i64,
    tokens_cache_escrita: i64,
    tokens_cache_leitura: i64,
    tokens_saida: i64,
) -> Option<CustoDetalhado> {
    let preco = preco_do_modelo(modelo)?;
    // Recebe a taxa como parâmetro em vez de fixá-la: assim o mesmo cálculo
    // (tokens / 1M * taxa) serve pras quatro taxas diferentes abaixo, sem
    // repetir a fórmula quatro vezes.
    let custo = |tokens: i64, taxa_por_mtok: f64| tokens as f64 / 1_000_000.0 * taxa_por_mtok;
    Some(CustoDetalhado {
        entrada: custo(tokens_entrada, preco.entrada_por_mtok),
        cache_escrita: custo(
            tokens_cache_escrita,
            preco.entrada_por_mtok * CACHE_ESCRITA_MULTIPLICADOR,
        ),
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
    let peso_entrada = tokens_entrada as f64;
    let peso_cache_escrita = tokens_cache_escrita as f64 * CACHE_ESCRITA_MULTIPLICADOR;
    let peso_cache_leitura = tokens_cache_leitura as f64 * CACHE_LEITURA_MULTIPLICADOR;
    let peso_saida = tokens_saida as f64 * RAZAO_SAIDA_ENTRADA;
    let peso_total = peso_entrada + peso_cache_escrita + peso_cache_leitura + peso_saida;

    if peso_total <= 0.0 {
        return CustoDetalhado {
            entrada: 0.0,
            cache_escrita: 0.0,
            cache_leitura: 0.0,
            saida: 0.0,
        };
    }

    let fator = custo_total / peso_total;
    CustoDetalhado {
        entrada: peso_entrada * fator,
        cache_escrita: peso_cache_escrita * fator,
        cache_leitura: peso_cache_leitura * fator,
        saida: peso_saida * fator,
    }
}

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
}
