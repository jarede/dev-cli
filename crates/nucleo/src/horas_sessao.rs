// NÚCLEO PURO: agregação de "horas trabalhadas" a partir de sessões já
// resolvidas (dia + duração). Extraído de `crates/cli/src/ai/render.rs`
// para ser reaproveitado tanto pelo `dev-cli ai stats claude/opencode`
// (CLI) quanto pelo endpoint `/api/ia/custos` do dev-server — as duas
// pontas calculam "quantas horas por semana/dia" exatamente da mesma
// forma, em vez de cada uma reimplementar o clamp e a soma por conta
// própria. `crates/cli/src/ai/render.rs` reexporta este módulo (ver
// `pub use nucleo::horas_sessao::*` lá) para o resto do CLI continuar
// escrevendo `render::Sessao` sem notar a mudança.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

/// Teto de duração de uma sessão contígua: sessões que ficam abertas a
/// noite toda não devem contar como horas de trabalho contínuo.
pub const TETO_HORAS: f64 = 4.0;
/// Piso de duração: mesmo uma sessão de 1 mensagem só (sem intervalo para
/// medir) conta um mínimo simbólico, em vez de zero.
pub const MINIMO_HORAS: f64 = 1.0 / 60.0;

/// Uma sessão de trabalho com data e duração em horas.
#[derive(Debug, Clone, PartialEq)]
pub struct Sessao {
    pub dia: NaiveDate,
    pub duracao_horas: f64,
}

/// Duração de uma sessão a partir dos horários de suas mensagens (já
/// ordenados). Sessão de 1 mensagem só vira um valor fixo de 5 minutos —
/// não há intervalo pra medir. Com 2+ horários, é a diferença entre o
/// primeiro e o último, limitada entre `MINIMO_HORAS` e `TETO_HORAS`.
pub fn duracao_sessao(horarios: &[DateTime<Utc>]) -> Option<f64> {
    let inicio = *horarios.first()?;
    if horarios.len() < 2 {
        return Some(5.0 / 60.0);
    }
    let fim = *horarios.last()?;
    let horas_brutas = (fim - inicio).num_seconds() as f64 / 3600.0;
    Some(horas_brutas.clamp(MINIMO_HORAS, TETO_HORAS))
}

/// Mapa dia -> (soma de horas, quantidade de sessões).
pub fn agregar_por_dia(sessoes: &[Sessao]) -> BTreeMap<NaiveDate, (f64, u32)> {
    let mut mapa: BTreeMap<NaiveDate, (f64, u32)> = BTreeMap::new();
    for sessao in sessoes {
        let entrada = mapa.entry(sessao.dia).or_insert((0.0, 0));
        entrada.0 += sessao.duracao_horas;
        entrada.1 += 1;
    }
    mapa
}

/// Mapa segunda-feira-da-semana -> (soma de horas, quantidade de sessões,
/// conjunto de dias distintos com atividade naquela semana).
pub fn agregar_por_semana(
    sessoes: &[Sessao],
) -> BTreeMap<NaiveDate, (f64, u32, BTreeSet<NaiveDate>)> {
    let mut mapa: BTreeMap<NaiveDate, (f64, u32, BTreeSet<NaiveDate>)> = BTreeMap::new();
    for sessao in sessoes {
        let segunda =
            sessao.dia - Duration::days(sessao.dia.weekday().num_days_from_monday() as i64);
        let entrada = mapa.entry(segunda).or_insert((0.0, 0, BTreeSet::new()));
        entrada.0 += sessao.duracao_horas;
        entrada.1 += 1;
        entrada.2.insert(sessao.dia);
    }
    mapa
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(segundos: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(segundos, 0).unwrap()
    }

    #[test]
    fn duracao_sessao_de_uma_mensagem_e_cinco_minutos() {
        assert_eq!(duracao_sessao(&[dt(0)]), Some(5.0 / 60.0));
    }

    #[test]
    fn duracao_sessao_lista_vazia_e_none() {
        assert_eq!(duracao_sessao(&[]), None);
    }

    #[test]
    fn duracao_sessao_clampa_no_teto() {
        // 10h de intervalo -> clampa em TETO_HORAS (4h).
        let horas = duracao_sessao(&[dt(0), dt(10 * 3600)]).unwrap();
        assert_eq!(horas, TETO_HORAS);
    }

    #[test]
    fn duracao_sessao_normal_e_a_diferenca() {
        let horas = duracao_sessao(&[dt(0), dt(3600)]).unwrap();
        assert_eq!(horas, 1.0);
    }

    #[test]
    fn agregar_por_dia_soma_sessoes_do_mesmo_dia() {
        let dia = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let sessoes = vec![
            Sessao {
                dia,
                duracao_horas: 1.0,
            },
            Sessao {
                dia,
                duracao_horas: 2.0,
            },
        ];
        let mapa = agregar_por_dia(&sessoes);
        assert_eq!(mapa[&dia], (3.0, 2));
    }

    #[test]
    fn agregar_por_semana_agrupa_na_segunda() {
        // 1/jul/2026 é uma quarta-feira; a segunda daquela semana é 29/jun.
        let quarta = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let segunda_esperada = NaiveDate::from_ymd_opt(2026, 6, 29).unwrap();
        let sessoes = vec![Sessao {
            dia: quarta,
            duracao_horas: 2.5,
        }];
        let mapa = agregar_por_semana(&sessoes);
        assert_eq!(mapa.len(), 1);
        let (horas, sessoes_qtd, dias) = &mapa[&segunda_esperada];
        assert_eq!(*horas, 2.5);
        assert_eq!(*sessoes_qtd, 1);
        assert!(dias.contains(&quarta));
    }
}
