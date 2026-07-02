// Câmbio USD -> BRL: parte de IO (chamada HTTP, sem teste automatizado,
// mesmo tratamento que a leitura de arquivos recebe em `src/logs.rs`) +
// uma função pura de conversão, essa sim testável.

use std::collections::HashMap;
use std::time::Duration;

use serde::Deserialize;

// Só o campo que nos interessa da resposta da API; `serde` ignora o resto
// do JSON automaticamente (não precisamos declarar cada campo da resposta).
#[derive(Deserialize)]
struct RespostaCambio {
    rates: HashMap<String, f64>,
}

// `reqwest::blocking` dá um cliente HTTP síncrono: por baixo dos panos usa
// um runtime assíncrono, mas a API que a gente vê é `Result` comum, sem
// `.await` nem `#[tokio::main]` — não precisamos tornar o resto da CLI
// assíncrona só por causa desta chamada.
pub fn buscar_taxa_usd_brl() -> Result<f64, Box<dyn std::error::Error>> {
    let cliente = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let resposta: RespostaCambio = cliente
        .get("https://api.frankfurter.dev/v1/latest?from=USD&to=BRL")
        .send()?
        .error_for_status()?
        .json()?;

    resposta
        .rates
        .get("BRL")
        .copied()
        .ok_or_else(|| "resposta da API de câmbio não trouxe a taxa BRL".into())
}

// Núcleo puro: dado um valor em USD e uma taxa, devolve o valor em BRL.
pub fn converter_para_brl(valor_usd: f64, taxa: f64) -> f64 {
    valor_usd * taxa
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converter_para_brl_multiplica_pela_taxa() {
        assert_eq!(converter_para_brl(10.0, 5.0), 50.0);
        assert_eq!(converter_para_brl(0.0, 5.0), 0.0);
    }
}
