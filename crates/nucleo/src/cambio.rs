// Câmbio USD -> BRL: parte de IO (chamada HTTP síncrona, sem teste
// automatizado, mesmo tratamento que o resto do IO do projeto) + uma função
// pura de conversão, essa sim testável.
//
// Extraído de `crates/cli/src/ai/cambio.rs` (achado 12 da revisão do
// redesign do portal) para o endpoint `/api/ia/cambio` do dev-server
// reaproveitar a MESMA busca ao vivo que o `dev-cli ai stats` já fazia, em
// vez de ficar com uma taxa fixa hardcoded enquanto o comentário dizia que
// "já existe em cambio.rs". `crates/cli/src/ai/cambio.rs` reexporta este
// módulo para o resto do CLI continuar chamando `cambio::buscar_taxa_usd_brl`
// sem notar a mudança.

use std::collections::HashMap;
use std::time::Duration;

// `Deserialize`: macro de derive do `serde` que gera, a partir dos campos
// anotados, o código que sabe transformar um JSON (bytes/texto) na struct
// Rust correspondente — não escrevemos esse parsing manualmente.
// docs: https://docs.rs/serde/latest/serde/trait.Deserialize.html
use serde::Deserialize;

// Só o campo que nos interessa da resposta da API; `serde` ignora o resto
// do JSON automaticamente. A API devolve algo como
// `{"rates": {"BRL": 5.42}, ...outros campos...}`; como as chaves de
// `rates` variam, um `HashMap<String, f64>` é o tipo certo.
// docs: https://doc.rust-lang.org/std/collections/struct.HashMap.html
#[derive(Deserialize)]
struct RespostaCambio {
    rates: HashMap<String, f64>,
}

// `reqwest::blocking` dá um cliente HTTP síncrono: por baixo dos panos usa
// um runtime assíncrono, mas a API que a gente vê é `Result` comum, sem
// `.await`. Quem chama esta função a partir de um contexto async (o
// endpoint `/api/ia/cambio` do dev-server) precisa envolvê-la em
// `tokio::task::spawn_blocking` — chamar código bloqueante direto dentro de
// um handler async travaria o executor do tokio para as OUTRAS requisições
// que compartilham a mesma thread.
// docs: https://docs.rs/reqwest/latest/reqwest/blocking/index.html
// docs: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html
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
// Não faz IO nem pode falhar, então não precisa de `Result`.
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
