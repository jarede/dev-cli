// RENDERIZAÇÃO: monta o texto colorido a partir das contagens,
// sem saber de onde elas vieram.

use std::collections::BTreeMap;

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

    if contagens.niveis.values().any(|&valor| valor > 0) {
        saida.push_str("   supervisord   ");
        let campos: Vec<String> = contagens
            .niveis
            .iter()
            .filter(|&(_, &contagem)| contagem > 0)
            .map(|(nivel, &contagem)| colorir_nivel(nivel, contagem))
            .collect();
        saida.push_str(&campos.join("   "));
        saida.push('\n');
    }

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
    let texto = format!("{nivel} {}", contagem.bold());
    match nivel.to_uppercase().as_str() {
        "ERROR" | "ERRO" | "CRIT" | "CRITICAL" | "FATAL" => texto.red().to_string(),
        "WARN" | "WARNING" => texto.yellow().to_string(),
        "INFO" => texto.green().to_string(),
        "DEBUG" | "DEBG" => texto.dimmed().to_string(),
        "TRAC" | "TRACE" => texto.cyan().to_string(),
        _ => texto,
    }
}
