// NÚCLEO PURO: estatísticas e classificação de severidade dos containers.
// Nenhuma função aqui faz IO — tudo recebe valores e devolve valores, o que
// permite testar 100% dos caminhos com dados inline.

use crate::config::Limiares;
use serde::Serialize;

/// Percentil 95 das durações (segundos). `None` para lista vazia.
/// Método "nearest-rank": ordena e pega o elemento na posição
/// ceil(0.95 * n) — simples e suficiente para um dashboard.
// `&[f64]` (slice) em vez de `&Vec<f64>`: aceita Vec, array, fatia... e
// deixa claro que só LEMOS os dados (o clone+sort acontece dentro).
// docs: https://doc.rust-lang.org/book/ch04-03-slices.html
pub fn p95(duracoes: &[f64]) -> Option<f64> {
    if duracoes.is_empty() {
        return None;
    }
    let mut ordenadas = duracoes.to_vec();
    // f64 não implementa `Ord` (NaN quebra a ordem total), então usamos
    // `sort_by` com `total_cmp`, que define uma ordem total para floats.
    // docs: https://doc.rust-lang.org/std/primitive.f64.html#method.total_cmp
    ordenadas.sort_by(|a, b| a.total_cmp(b));
    let n = ordenadas.len();
    let posicao = ((n as f64) * 0.95).ceil() as usize;
    // `saturating_sub(1)`: converte posição 1-based em índice 0-based sem
    // risco de underflow quando n == 1.
    // docs: https://doc.rust-lang.org/std/primitive.usize.html#method.saturating_sub
    Some(ordenadas[posicao.saturating_sub(1).min(n - 1)])
}

/// Tudo que o dashboard mostra sobre um container, já agregado na janela.
// `Serialize`: a Fase 2 (dev-server) devolve esta struct como JSON sem DTO
// intermediário — os nomes dos campos viram as chaves do JSON.
// docs: https://serde.rs/derive.html
#[derive(Debug, Clone, Default, Serialize)]
pub struct ResumoContainer {
    pub nome: String,
    /// "running" ou "stopped" (coluna `status` da tabela containers).
    pub status: String,
    /// Texto de uptime do docker ps ("Up 2 days"); vazio se desconhecido.
    pub uptime: String,
    /// Linhas de nível ERROR/ERRO na janela.
    pub erros: i64,
    /// Linhas de nível CRITICAL/CRIT/FATAL na janela.
    pub crits: i64,
    /// Requests HTTP com status 5xx na janela.
    pub c5xx: i64,
    /// Requests HTTP com status 4xx na janela.
    pub c4xx: i64,
    /// Total de requests HTTP na janela.
    pub reqs: i64,
    /// p95 do tempo de resposta (segundos) na janela; None sem requests.
    pub p95_seg: Option<f64>,
    /// Maior tempo de resposta (segundos) na janela.
    pub max_seg: Option<f64>,
    /// Total de linhas de log (todos os níveis) na janela.
    pub total_linhas: i64,
    /// Timestamp Unix da última coleta deste container.
    pub ultima_coleta: i64,
}

/// Severidade de um container, da melhor para a pior.
// A ORDEM das variantes importa: `derive(Ord)` ordena pela posição de
// declaração, então Verde < Amarelo < Vermelho < Parado — o dashboard
// ordena decrescente para pôr os piores no topo.
// docs: https://doc.rust-lang.org/std/cmp/trait.Ord.html#derivable
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Severidade {
    Verde,
    Amarelo,
    Vermelho,
    Parado,
}

/// Classifica um container:
///   Parado    — não está mais rodando (gravidade máxima);
///   Vermelho  — CRIT presente, OU taxa de ERROR/CRIT ≥ limiar, OU algum 5xx;
///   Amarelo   — p95 ≥ limiar de lentidão, OU há erros abaixo do limiar,
///               OU mais de 10% das requests são 4xx;
///   Verde     — nada acima.
pub fn severidade(resumo: &ResumoContainer, limiares: &Limiares) -> Severidade {
    if resumo.status == "stopped" {
        return Severidade::Parado;
    }

    let taxa_erro = if resumo.total_linhas > 0 {
        ((resumo.erros + resumo.crits) as f64) * 100.0 / (resumo.total_linhas as f64)
    } else {
        0.0
    };
    if resumo.crits > 0 || resumo.c5xx > 0 || taxa_erro >= limiares.taxa_erro_pct {
        return Severidade::Vermelho;
    }

    // Let chain (edition 2024): `Some(p)` E a condição, sem `if` aninhado.
    if let Some(p) = resumo.p95_seg
        && p >= limiares.p95_lento_seg
    {
        return Severidade::Amarelo;
    }
    let muitos_4xx = resumo.reqs > 0 && (resumo.c4xx as f64) > (resumo.reqs as f64) * 0.10;
    if resumo.erros > 0 || muitos_4xx {
        return Severidade::Amarelo;
    }

    Severidade::Verde
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiares() -> Limiares {
        Limiares {
            p95_lento_seg: 1.0,
            taxa_erro_pct: 5.0,
        }
    }

    fn resumo_saudavel() -> ResumoContainer {
        ResumoContainer {
            nome: "app".to_string(),
            status: "running".to_string(),
            total_linhas: 100,
            reqs: 50,
            p95_seg: Some(0.2),
            max_seg: Some(0.5),
            ..Default::default()
        }
    }

    #[test]
    fn p95_de_lista_vazia_e_none() {
        assert_eq!(p95(&[]), None);
    }

    #[test]
    fn p95_de_um_elemento_e_ele_mesmo() {
        assert_eq!(p95(&[0.42]), Some(0.42));
    }

    #[test]
    fn p95_de_cem_elementos_e_o_95o() {
        // 0.01, 0.02, ..., 1.00 -> o 95º valor é 0.95
        let valores: Vec<f64> = (1..=100).map(|i| i as f64 / 100.0).collect();
        assert_eq!(p95(&valores), Some(0.95));
    }

    #[test]
    fn p95_nao_depende_da_ordem_de_entrada() {
        assert_eq!(p95(&[3.0, 1.0, 2.0]), Some(3.0));
    }

    #[test]
    fn container_saudavel_e_verde() {
        assert_eq!(severidade(&resumo_saudavel(), &limiares()), Severidade::Verde);
    }

    #[test]
    fn container_parado_e_parado_mesmo_sem_erros() {
        let mut r = resumo_saudavel();
        r.status = "stopped".to_string();
        assert_eq!(severidade(&r, &limiares()), Severidade::Parado);
    }

    #[test]
    fn crit_e_vermelho() {
        let mut r = resumo_saudavel();
        r.crits = 1;
        assert_eq!(severidade(&r, &limiares()), Severidade::Vermelho);
    }

    #[test]
    fn cincoxx_e_vermelho() {
        let mut r = resumo_saudavel();
        r.c5xx = 1;
        assert_eq!(severidade(&r, &limiares()), Severidade::Vermelho);
    }

    #[test]
    fn taxa_de_erro_acima_do_limiar_e_vermelho() {
        let mut r = resumo_saudavel();
        r.erros = 6; // 6 de 100 linhas = 6% >= 5%
        assert_eq!(severidade(&r, &limiares()), Severidade::Vermelho);
    }

    #[test]
    fn poucos_erros_abaixo_do_limiar_e_amarelo() {
        let mut r = resumo_saudavel();
        r.erros = 2; // 2% < 5%
        assert_eq!(severidade(&r, &limiares()), Severidade::Amarelo);
    }

    #[test]
    fn p95_lento_e_amarelo() {
        let mut r = resumo_saudavel();
        r.p95_seg = Some(1.5);
        assert_eq!(severidade(&r, &limiares()), Severidade::Amarelo);
    }

    #[test]
    fn muitos_4xx_e_amarelo() {
        let mut r = resumo_saudavel();
        r.c4xx = 10; // 10 de 50 = 20% > 10%
        assert_eq!(severidade(&r, &limiares()), Severidade::Amarelo);
    }

    #[test]
    fn severidade_ordena_do_verde_ao_parado() {
        assert!(Severidade::Verde < Severidade::Amarelo);
        assert!(Severidade::Amarelo < Severidade::Vermelho);
        assert!(Severidade::Vermelho < Severidade::Parado);
    }

    #[test]
    fn resumo_e_severidade_serializam_para_json() {
        // A API da Fase 2 devolve essas structs direto como JSON — este
        // teste trava o formato (nomes de campo em pt-br, enum como string).
        let json = serde_json::to_value(resumo_saudavel()).unwrap();
        assert_eq!(json["nome"], "app");
        assert_eq!(json["status"], "running");
        assert_eq!(json["p95_seg"], 0.2);
        assert_eq!(serde_json::to_value(Severidade::Parado).unwrap(), "Parado");
    }
}
