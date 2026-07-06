// NÚCLEO PURO: funções que processam texto de log sem efeitos colaterais.
// Todas recebem `&str` e devolvem dados; nenhuma toca em disco, rede ou terminal.

use std::collections::BTreeMap;

// Níveis que o supervisord escreve na 3ª coluna de cada linha própria.
const NIVEIS_CONHECIDOS: [&str; 6] = ["INFO", "DEBG", "WARN", "CRIT", "ERRO", "TRAC"];
// Palavras que procuramos no TEXTO da linha (logs da app embutidos no stdout).
const PALAVRAS_CHAVE: [&str; 4] = ["error", "warn", "info", "debug"];

// Níveis que a saída de `container logs` costuma usar.
const NIVEIS_CONTAINER: [&str; 8] = [
    "TRACE", "DEBUG", "INFO", "WARN", "WARNING", "ERROR", "CRITICAL", "FATAL",
];

// Níveis que `docker logs` pode conter — mescla dos formatos supervisor
// (DEBG, CRIT, ERRO, TRAC) com os formatos de app (DEBUG, INFO, WARN, ERROR).
const NIVEIS_DOCKER: [&str; 12] = [
    "TRACE", "TRAC", "DEBUG", "DEBG", "INFO", "WARN", "WARNING", "ERROR", "ERRO", "CRITICAL",
    "CRIT", "FATAL",
];

/// Resultado do núcleo puro para o formato supervisord:
/// dois mapas de "rótulo -> quantidade".
#[derive(Default, Debug, PartialEq)]
pub(crate) struct Contagens {
    pub(crate) niveis: BTreeMap<String, usize>,
    pub(crate) palavras: BTreeMap<String, usize>,
}

/// NÚCLEO PURO: conta ocorrências de níveis supervisord e palavras-chave
/// no texto de uma única passada, sem nenhum efeito colateral.
pub(crate) fn contar(conteudo: &str) -> Contagens {
    let mut contagens = Contagens::default();

    for linha in conteudo.lines() {
        if let Some(token) = linha.split_whitespace().nth(2)
            && NIVEIS_CONHECIDOS.contains(&token)
        {
            *contagens.niveis.entry(token.to_string()).or_insert(0) += 1;
        }

        let linha_minuscula = linha.to_lowercase();
        for palavra in PALAVRAS_CHAVE {
            if linha_minuscula.contains(palavra) {
                *contagens.palavras.entry(palavra.to_string()).or_insert(0) += 1;
            }
        }
    }

    contagens
}

/// NÚCLEO PURO: conta ocorrências de cada nível no formato `container logs`
/// (2º token de cada linha, após remover códigos ANSI).
pub(crate) fn contar_niveis_container(conteudo: &str) -> BTreeMap<String, usize> {
    let mut niveis = BTreeMap::new();
    for linha in conteudo.lines() {
        let limpa = remover_ansi(linha);
        if let Some(token) = limpa.split_whitespace().nth(1) {
            let token_maiusculo = token.to_uppercase();
            if NIVEIS_CONTAINER.contains(&token_maiusculo.as_str()) {
                *niveis.entry(token_maiusculo).or_insert(0) += 1;
            }
        }
    }
    niveis
}

/// NÚCLEO PURO: conta ocorrências de níveis de log numa string, procurando
/// em qualquer token da linha. Funciona com supervisord, container logs e
/// formatos livres.
pub(crate) fn contar_niveis_docker(conteudo: &str) -> BTreeMap<String, usize> {
    let mut niveis = BTreeMap::new();
    for linha in conteudo.lines() {
        let limpa = remover_ansi(linha);
        if let Some(nivel) = limpa
            .split_whitespace()
            .find(|token| NIVEIS_DOCKER.contains(&token.to_uppercase().as_str()))
        {
            *niveis.entry(nivel.to_uppercase()).or_insert(0) += 1;
        }
    }
    niveis
}

/// NÚCLEO PURO: categoriza cada linha de log pelo nível detectado.
/// Devolve um mapa de nível → lista de linhas (já sem códigos ANSI).
pub(crate) fn categorizar_por_nivel(conteudo: &str) -> BTreeMap<String, Vec<String>> {
    let mut grupos: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for linha in conteudo.lines() {
        let limpa = remover_ansi(linha);
        if let Some(nivel) = limpa
            .split_whitespace()
            .find(|token| NIVEIS_DOCKER.contains(&token.to_uppercase().as_str()))
        {
            grupos.entry(nivel.to_uppercase()).or_default().push(limpa);
        }
    }
    grupos
}

/// Remove sequências de escape ANSI (ex.: "\x1b[32m") de uma linha,
/// deixando só o texto visível.
pub(crate) fn remover_ansi(linha: &str) -> String {
    let mut limpa = String::with_capacity(linha.len());
    let mut chars = linha.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for ch in chars.by_ref() {
                if ch == 'm' {
                    break;
                }
            }
        } else {
            limpa.push(c);
        }
    }
    limpa
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conta_niveis_supervisord_e_ignora_continuacoes() {
        let conteudo = "\
2026-05-08 14:29:39,438 INFO mensagem qualquer
2026-05-08 14:29:39,438 DEBG mensagem com Error no meio
2026-05-08 14:29:39,438 WARN aviso
2026-05-08 14:29:39,438 CRIT falha grave
2026-05-08 14:29:39,438 ERRO erro explícito
2026-05-08 14:29:39,438 TRAC rastreio detalhado
linha de continuação sem timestamp nem nível";

        let contagens = contar(conteudo);

        assert_eq!(contagens.niveis.get("INFO"), Some(&1));
        assert_eq!(contagens.niveis.get("DEBG"), Some(&1));
        assert_eq!(contagens.niveis.get("WARN"), Some(&1));
        assert_eq!(contagens.niveis.get("CRIT"), Some(&1));
        assert_eq!(contagens.niveis.get("ERRO"), Some(&1));
        assert_eq!(contagens.niveis.get("TRAC"), Some(&1));
        assert_eq!(contagens.niveis.len(), 6);
    }

    #[test]
    fn linha_de_continuacao_nao_conta_nivel() {
        let conteudo = "isso não tem timestamp nem nível válido na posição 3";
        let contagens = contar(conteudo);
        assert!(contagens.niveis.is_empty());
    }

    #[test]
    fn palavra_chave_error_e_case_insensitive() {
        let conteudo = "\
2026-05-08 14:29:39,438 DEBG contém Error no texto
2026-05-08 14:29:39,438 DEBG contém ERROR no texto
2026-05-08 14:29:39,438 DEBG contém error no texto";

        let contagens = contar(conteudo);

        assert_eq!(contagens.palavras.get("error"), Some(&3));
    }

    #[test]
    fn conta_uma_vez_por_linha_mesmo_com_multiplas_ocorrencias() {
        let conteudo = "2026-05-08 14:29:39,438 INFO error error error na mesma linha";
        let contagens = contar(conteudo);
        assert_eq!(contagens.palavras.get("error"), Some(&1));
    }

    #[test]
    fn remove_ansi_preserva_apenas_texto_visivel() {
        let linha =
            "\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: msg";
        assert_eq!(remover_ansi(linha), "2026-07-03  INFO dev_web: msg");
    }

    #[test]
    fn conta_niveis_container_ignora_codigos_ansi() {
        let conteudo = "\
\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: server starting
\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[31mERROR\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: falha ao conectar
\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[33m WARN\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: lento
\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[35mCRITICAL\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: crash iminente";

        let niveis = contar_niveis_container(conteudo);

        assert_eq!(niveis.get("INFO"), Some(&1));
        assert_eq!(niveis.get("ERROR"), Some(&1));
        assert_eq!(niveis.get("WARN"), Some(&1));
        assert_eq!(niveis.get("CRITICAL"), Some(&1));
        assert_eq!(niveis.len(), 4);
    }

    #[test]
    fn conta_niveis_container_ignora_linhas_sem_nivel_conhecido() {
        let conteudo = "mensagem qualquer sem nivel reconhecivel no segundo token";
        let niveis = contar_niveis_container(conteudo);
        assert!(niveis.is_empty());
    }
}
