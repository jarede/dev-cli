// RENDERIZAÇÃO: monta o texto colorido a partir das contagens,
// sem saber de onde elas vieram.

use std::collections::BTreeMap;

// Trait de extensão do `owo-colors`: ao importá-la, todo tipo que implementa
// `Display` ganha métodos como `.red()`, `.bold()`, `.dimmed()`.
// docs: https://docs.rs/owo-colors/latest/owo_colors/trait.OwoColorize.html
use owo_colors::OwoColorize;

use crate::logs::core::Contagens;

/// Monta o bloco de texto (colorido) com os níveis de um container.
/// Recebe um `BTreeMap` "cru" (em vez do `struct Contagens`) porque aqui
/// só existe uma dimensão de contagem (níveis) — não há a segunda dimensão
/// "palavras-chave no texto" que o modo `stats` tem.
pub(crate) fn renderizar_container(
    nome: &str,
    status: Option<&str>,
    niveis: &BTreeMap<String, usize>,
) -> String {
    let cabecalho = if let Some(s) = status
        && !s.is_empty()
    {
        format!("📦 {}  ({})", nome.bold(), s.dimmed())
    } else {
        format!("📦 {}", nome.bold())
    };
    let mut saida = format!("{cabecalho}\n");

    // `any`: verdadeiro se existir ao menos uma entrada com contagem > 0;
    // pára na primeira que satisfizer, sem percorrer o mapa inteiro à toa.
    if niveis.values().any(|&valor| valor > 0) {
        saida.push_str("   ");
        let campos: Vec<String> = niveis
            .iter()
            .filter(|&(_, &contagem)| contagem > 0)
            .map(|(nivel, &contagem)| colorir_nivel(nivel, contagem))
            .collect();
        saida.push_str(&campos.join("   "));
        saida.push('\n');
    } else {
        saida.push_str("   (nenhum nível reconhecido nos logs)\n");
    }

    saida
}

/// Monta o bloco de texto (colorido) de um container no formato supervisord.
pub(crate) fn renderizar(nome: &str, contagens: &Contagens) -> String {
    let mut saida = format!("📦 {}\n", nome.bold());

    // Só imprime a linha "supervisord" se houver ao menos uma contagem > 0.
    // `any` para na primeira que satisfaz a condição.
    if contagens.niveis.values().any(|&valor| valor > 0) {
        saida.push_str("   supervisord   ");
        // Coleta cada nível não-zero já colorido numa lista de Strings.
        // O padrão `|&(_, &contagem)|` desestrutura a referência da tupla:
        // `_` ignora a chave e `&contagem` copia o número emprestado.
        let campos: Vec<String> = contagens
            .niveis
            .iter()
            .filter(|&(_, &contagem)| contagem > 0)
            .map(|(nivel, &contagem)| colorir_nivel(nivel, contagem))
            .collect();
        // `join` intercala os campos com espaços, sem separador sobrando na ponta.
        saida.push_str(&campos.join("   "));
        saida.push('\n');
    }

    // Mesma lógica para as palavras-chave encontradas no texto.
    if contagens.palavras.values().any(|&valor| valor > 0) {
        saida.push_str("   texto         ");
        let campos: Vec<String> = contagens
            .palavras
            .iter()
            .filter(|&(_, &contagem)| contagem > 0)
            .map(|(palavra, &contagem)| colorir_nivel(palavra, contagem))
            .collect();
        saida.push_str(&campos.join("   "));
        saida.push('\n');
    }

    saida
}

/// Escolhe a cor conforme a severidade e devolve o campo já formatado.
fn colorir_nivel(nivel: &str, contagem: usize) -> String {
    // O número em negrito; `{}` usa o `Display` que o `.bold()` produz.
    let texto = format!("{nivel} {}", contagem.bold());
    // Comparamos em maiúsculo para casar tanto "ERRO"/"error" quanto "INFO"/"info".
    match nivel.to_uppercase().as_str() {
        "ERROR" | "ERRO" | "CRIT" | "CRITICAL" | "FATAL" => texto.red().to_string(),
        "WARN" | "WARNING" => texto.yellow().to_string(),
        "INFO" => texto.green().to_string(),
        "DEBUG" | "DEBG" => texto.dimmed().to_string(),
        "TRAC" | "TRACE" => texto.cyan().to_string(),
        // `_` é o caso-curinga: qualquer outro rótulo fica sem cor.
        _ => texto,
    }
}
