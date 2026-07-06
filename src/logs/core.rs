// NÚCLEO PURO: funções que processam texto de log sem efeitos colaterais.
// Todas recebem `&str` e devolvem dados; nenhuma toca em disco, rede ou terminal.
// Por serem puras, são 100% testáveis com strings inline (ver módulo `tests`).

use std::collections::BTreeMap;

// Níveis que o supervisord escreve na 3ª coluna de cada linha própria.
// `[&str; 6]` é um array de tamanho fixo conhecido em tempo de compilação.
const NIVEIS_CONHECIDOS: [&str; 6] = ["INFO", "DEBG", "WARN", "CRIT", "ERRO", "TRAC"];
// Palavras que procuramos no TEXTO da linha (logs da app embutidos no stdout).
const PALAVRAS_CHAVE: [&str; 4] = ["error", "warn", "info", "debug"];

// Níveis que a saída de `container logs` costuma usar (formatos ao estilo
// das crates `tracing`/`log`, ou de logging do Python). Comparamos sempre em
// maiúsculo, então "warn" e "WARN" caem na mesma chave.
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
// `#[derive(Default)]` dá um construtor `Contagens::default()` com mapas vazios;
// `PartialEq` permite comparar com `assert_eq!` nos testes.
#[derive(Default, Debug, PartialEq)]
pub(crate) struct Contagens {
    pub(crate) niveis: BTreeMap<String, usize>, // coluna de nível do supervisord
    pub(crate) palavras: BTreeMap<String, usize>, // palavras-chave encontradas no texto
}

/// NÚCLEO PURO: conta ocorrências de níveis supervisord e palavras-chave
/// no texto numa única passada, sem nenhum efeito colateral.
pub(crate) fn contar(conteudo: &str) -> Contagens {
    let mut contagens = Contagens::default();

    // `lines()` itera linha a linha sem alocar cópias (empresta fatias do texto).
    for linha in conteudo.lines() {
        // Coluna supervisord: `split_whitespace()` quebra por espaços e `nth(2)`
        // pega o 3º token (índice 2). Retorna `Option`: `None` se não existir.
        // A "let chain" (`&& ...`) só entra no bloco se o token existir E for um
        // nível conhecido (`contains` sobre o array).
        if let Some(token) = linha.split_whitespace().nth(2)
            && NIVEIS_CONHECIDOS.contains(&token)
        {
            // API `entry`: pega o contador da chave (inserindo 0 se ausente)
            // e o `*... += 1` incrementa o valor no lugar.
            *contagens.niveis.entry(token.to_string()).or_insert(0) += 1;
        }

        // Busca de palavras-chave case-insensitive: comparamos tudo em minúsculo.
        let linha_minuscula = linha.to_lowercase();
        for palavra in PALAVRAS_CHAVE {
            // Conta 1 por LINHA que contém a palavra (não por ocorrência).
            if linha_minuscula.contains(palavra) {
                *contagens.palavras.entry(palavra.to_string()).or_insert(0) += 1;
            }
        }
    }

    contagens
}

/// NÚCLEO PURO: conta ocorrências de cada nível na 2ª coluna de cada linha
/// (depois da data), já sem códigos ANSI. Também é chamada linha a linha pelo
/// modo `-f` (uma linha por vez), então precisa funcionar tanto para um texto
/// gigante quanto para uma única linha recém-chegada.
pub(crate) fn contar_niveis_container(conteudo: &str) -> BTreeMap<String, usize> {
    let mut niveis = BTreeMap::new();
    for linha in conteudo.lines() {
        let limpa = remover_ansi(linha);
        // Índice 1 (2º token) = nível: o formato de `container logs` é
        // "<data> <nível> <origem>: <mensagem>", sem hora junto da data.
        if let Some(token) = limpa.split_whitespace().nth(1) {
            // `to_uppercase()` normaliza antes de comparar/guardar, para que
            // "info" e "INFO" caiam sempre na mesma chave do mapa.
            let token_maiusculo = token.to_uppercase();
            if NIVEIS_CONTAINER.contains(&token_maiusculo.as_str()) {
                // API `entry`: busca a chave `token_maiusculo` no mapa e, se
                // ainda não existir, insere `0` antes de devolver a referência
                // mutável ao valor (`or_insert(0)`). O `*` desreferencia essa
                // referência para poder somar `1` no lugar, numa única
                // expressão em vez de "verificar se existe, senão criar, senão
                // incrementar".
                *niveis.entry(token_maiusculo).or_insert(0) += 1;
            }
        }
    }
    niveis
}

/// NÚCLEO PURO: conta ocorrências de níveis de log numa string, procurando
/// em qualquer token da linha (não apenas numa posição fixa). Funciona com
/// os formatos do supervisor ("2026-07-06 09:05:11,722 DEBG ..."), do
/// container logs ("2026-07-03 INFO ...") e de apps que logam no formato
/// livre.
pub(crate) fn contar_niveis_docker(conteudo: &str) -> BTreeMap<String, usize> {
    let mut niveis = BTreeMap::new();
    for linha in conteudo.lines() {
        let limpa = remover_ansi(linha);
        // Varre todos os tokens whitespace-delimited em busca de um nível
        // conhecido. Usamos `any` que para no primeiro match (1 nível por
        // linha, mesmo que a linha contenha múltiplas ocorrências).
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
            // `entry(...).or_default()`: se o nível ainda não é chave do mapa,
            // insere um `Vec` vazio (o `Default` de `Vec<String>`); em
            // seguida `.push(limpa)` empilha a linha nesse `Vec`, seja ele
            // recém-criado ou já existente.
            // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.entry
            // docs: https://doc.rust-lang.org/std/collections/btree_map/enum.Entry.html#method.or_default
            // docs: https://doc.rust-lang.org/std/vec/struct.Vec.html#method.push
            grupos.entry(nivel.to_uppercase()).or_default().push(limpa);
        }
    }
    grupos
}

/// Remove sequências de escape ANSI (ex.: "\x1b[32m") de uma linha, deixando
/// só o texto visível. `container logs` colore a saída, o que atrapalharia a
/// busca pelo token do nível se não fosse removido antes.
pub(crate) fn remover_ansi(linha: &str) -> String {
    // `with_capacity` pré-aloca o buffer no tamanho da linha original: como
    // só removemos caracteres, o resultado nunca é maior, evitando
    // realocações durante os `push`.
    let mut limpa = String::with_capacity(linha.len());
    // `chars()` é um iterador sobre os `char` (Unicode) da string; guardamos
    // ele numa variável `mut` porque o `for` interno (`chars.by_ref()`)
    // precisa avançá-lo manualmente, então não pode ser um `for` simples aqui.
    let mut chars = linha.chars();
    // `while let Some(c) = chars.next()`: repete enquanto o iterador ainda
    // devolver itens; equivale a um `for c in chars` de baixo nível, mas
    // permite avançar o iterador "extra" dentro do laço (no `by_ref()` abaixo).
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Descarta tudo até o 'm' que fecha o código de escape (formato
            // "\x1b[<códigos>m"); `by_ref()` evita mover o iterador inteiro.
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
    // Traz para o escopo tudo do módulo pai (incluindo `contar` e `Contagens`).
    use super::*;

    #[test]
    fn conta_niveis_supervisord_e_ignora_continuacoes() {
        // `"\` no fim da linha junta as linhas seguintes sem espaço extra.
        let conteudo = "\
2026-05-08 14:29:39,438 INFO mensagem qualquer
2026-05-08 14:29:39,438 DEBG mensagem com Error no meio
2026-05-08 14:29:39,438 WARN aviso
2026-05-08 14:29:39,438 CRIT falha grave
2026-05-08 14:29:39,438 ERRO erro explícito
2026-05-08 14:29:39,438 TRAC rastreio detalhado
linha de continuação sem timestamp nem nível";

        let contagens = contar(conteudo);

        // Cada nível apareceu exatamente uma vez.
        assert_eq!(contagens.niveis.get("INFO"), Some(&1));
        assert_eq!(contagens.niveis.get("DEBG"), Some(&1));
        assert_eq!(contagens.niveis.get("WARN"), Some(&1));
        assert_eq!(contagens.niveis.get("CRIT"), Some(&1));
        assert_eq!(contagens.niveis.get("ERRO"), Some(&1));
        assert_eq!(contagens.niveis.get("TRAC"), Some(&1));
        // A linha de continuação não adicionou nenhuma chave nova.
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

        // Error / ERROR / error contam todos para a mesma chave "error".
        assert_eq!(contagens.palavras.get("error"), Some(&3));
    }

    #[test]
    fn conta_uma_vez_por_linha_mesmo_com_multiplas_ocorrencias() {
        let conteudo = "2026-05-08 14:29:39,438 INFO error error error na mesma linha";
        let contagens = contar(conteudo);
        // Três ocorrências na mesma linha = 1 (contamos por linha).
        assert_eq!(contagens.palavras.get("error"), Some(&1));
    }

    #[test]
    fn remove_ansi_preserva_apenas_texto_visivel() {
        // Sequência real emitida por `container logs`: "\x1b[32m INFO\x1b[0m".
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
