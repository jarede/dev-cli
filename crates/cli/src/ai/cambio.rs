// Câmbio USD -> BRL: parte de IO (chamada HTTP, sem teste automatizado,
// mesmo tratamento que a leitura de arquivos recebe em `src/logs.rs`) +
// uma função pura de conversão, essa sim testável.

use std::collections::HashMap;
use std::time::Duration;

// `Deserialize`: macro de derive do `serde` que gera, a partir dos campos
// anotados, o código que sabe transformar um JSON (bytes/texto) na struct
// Rust correspondente — não escrevemos esse parsing manualmente.
// docs: https://docs.rs/serde/latest/serde/trait.Deserialize.html
use serde::Deserialize;

// Só o campo que nos interessa da resposta da API; `serde` ignora o resto
// do JSON automaticamente (não precisamos declarar cada campo da resposta).
// A API devolve algo como `{"rates": {"BRL": 5.42}, ...outros campos...}`;
// como as chaves de `rates` variam (dependem de quais moedas foram pedidas),
// um `HashMap<String, f64>` é o tipo certo — não dá para usar uma struct com
// campo fixo, já que não sabemos os nomes das chaves em tempo de compilação.
// docs: https://doc.rust-lang.org/std/collections/struct.HashMap.html
#[derive(Deserialize)]
struct RespostaCambio {
    rates: HashMap<String, f64>,
}

// `reqwest::blocking` dá um cliente HTTP síncrono: por baixo dos panos usa
// um runtime assíncrono, mas a API que a gente vê é `Result` comum, sem
// `.await` nem `#[tokio::main]` — não precisamos tornar o resto da CLI
// assíncrona só por causa desta chamada.
// docs: https://docs.rs/reqwest/latest/reqwest/blocking/index.html
pub fn buscar_taxa_usd_brl() -> Result<f64, Box<dyn std::error::Error>> {
    // `Client::builder()` monta o cliente HTTP de forma configurável antes de
    // criá-lo; aqui só ajustamos o timeout (evita ficar esperando para sempre
    // se a API não responder). O `?` propaga o erro se a configuração falhar.
    // docs: https://docs.rs/reqwest/latest/reqwest/blocking/struct.Client.html#method.builder
    let cliente = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    // Encadeamento de `?`: cada etapa pode falhar e cada uma propaga o erro
    // para quem chamou esta função, sem precisarmos de `match` aninhado.
    //   .get(url)            -> monta a requisição (ainda não envia nada)
    //   .send()?             -> envia de fato e espera a resposta
    //   .error_for_status()? -> transforma status HTTP de erro (4xx/5xx) em
    //                           `Err`, já que `send()` sozinho não falha nesse caso
    //   .json()?             -> lê o corpo e desserializa para `RespostaCambio`,
    //                           usando o `#[derive(Deserialize)]` da struct acima
    // docs: https://docs.rs/reqwest/latest/reqwest/blocking/struct.Client.html#method.get
    // docs: https://docs.rs/reqwest/latest/reqwest/blocking/struct.RequestBuilder.html#method.send
    // docs: https://docs.rs/reqwest/latest/reqwest/blocking/struct.Response.html#method.error_for_status
    // docs: https://docs.rs/reqwest/latest/reqwest/blocking/struct.Response.html#method.json
    let resposta: RespostaCambio = cliente
        .get("https://api.frankfurter.dev/v1/latest?from=USD&to=BRL")
        .send()?
        .error_for_status()?
        .json()?;

    // `.get("BRL")` no HashMap devolve `Option<&f64>`; `.copied()` transforma
    // em `Option<f64>` (copia o valor em vez de manter o empréstimo, já que
    // `f64` é barato de copiar). `.ok_or_else(...)` converte o `None` em
    // `Err` com uma mensagem específica, só construindo a String se precisar
    // (por isso é uma closure, e não o valor já pronto como em `ok_or`).
    // docs: https://doc.rust-lang.org/std/collections/struct.HashMap.html#method.get
    // docs: https://doc.rust-lang.org/std/option/enum.Option.html#method.copied
    // docs: https://doc.rust-lang.org/std/option/enum.Option.html#method.ok_or_else
    resposta
        .rates
        .get("BRL")
        .copied()
        .ok_or_else(|| "resposta da API de câmbio não trouxe a taxa BRL".into())
}

// Núcleo puro: dado um valor em USD e uma taxa, devolve o valor em BRL.
// Não faz IO nem pode falhar, então não precisa de `Result`; por isso é
// trivialmente testável com valores inline (ver módulo `tests` abaixo).
pub fn converter_para_brl(valor_usd: f64, taxa: f64) -> f64 {
    valor_usd * taxa
}

// `#[cfg(test)]`: este módulo só é compilado ao rodar `cargo test`, não entra
// no binário final. Só testamos `converter_para_brl` (núcleo puro) — a
// chamada HTTP de `buscar_taxa_usd_brl` não tem teste automatizado aqui,
// mesmo tratamento dado à parte de IO em `src/logs.rs`.
// docs: https://doc.rust-lang.org/reference/conditional-compilation.html#the-test-attribute
#[cfg(test)]
mod tests {
    // Traz para o escopo tudo do módulo pai, incluindo `converter_para_brl`.
    // docs: https://doc.rust-lang.org/reference/items/use-declarations.html
    use super::*;

    #[test]
    fn converter_para_brl_multiplica_pela_taxa() {
        assert_eq!(converter_para_brl(10.0, 5.0), 50.0);
        assert_eq!(converter_para_brl(0.0, 5.0), 0.0);
    }
}
