// NÚCLEO PURO: funções que processam texto de log sem efeitos colaterais.
// Todas recebem `&str` e devolvem dados; nenhuma toca em disco, rede ou terminal.
// Por serem puras, são 100% testáveis com strings inline (ver módulo `tests`).

use std::collections::BTreeMap;
use std::fmt;

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

// Tipos de aplicação que detectamos nas linhas de log. Cada container roda
// uma app que loga num formato específico; identificar o formato permite
// filtrar por app dentro de um container que tem múltiplas apps.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum AppType {
    /// Uvicorn: "INFO:     message" (nível + dois-pontos no início da linha).
    Uvicorn,
    /// Elefante/Loguru: "YYYY-MM-DD HH:mm:ss.SSS | LEVEL | ..." ou linhas
    /// começando com "[" (legado logging.basicConfig).
    Elefante,
    /// Qualquer linha que não casa com uvicorn nem loguru (supervisord, etc.).
    Outros,
}

impl fmt::Display for AppType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppType::Uvicorn => write!(f, "Uvicorn"),
            AppType::Elefante => write!(f, "Elefante"),
            AppType::Outros => write!(f, "Outros"),
        }
    }
}

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

/// Normaliza um token de log removendo caracteres não alfanuméricos das
/// pontas (`|INFO:` → `INFO`, `:WARN,` → `WARN`, etc.) antes de comparar
/// com a lista de níveis conhecidos. Formatos como Loguru ("|INFO     |")
/// e uvicorn ("INFO:") usam pipe e dois-pontos ao redor do nível.
fn token_eh_nivel(token: &str) -> bool {
    let normalizado = token.trim_matches(|c: char| !c.is_ascii_alphanumeric());
    NIVEIS_DOCKER.contains(&normalizado.to_uppercase().as_str())
}

/// Extrai o nível de um token já normalizado (remove caracteres não
/// alfanuméricos das pontas e retorna em maiúsculo).
fn extrair_nivel(token: &str) -> String {
    token
        .trim_matches(|c: char| !c.is_ascii_alphanumeric())
        .to_uppercase()
}

/// NÚCLEO PURO: conta ocorrências de níveis de log numa string, procurando
/// em qualquer token da linha (não apenas numa posição fixa). Funciona com
/// os formatos do supervisor ("2026-07-06 09:05:11,722 DEBG ..."), do
/// container logs ("2026-07-03 INFO ..."), do Loguru ("|INFO     |...")
/// e do uvicorn ("INFO:     ...").
pub(crate) fn contar_niveis_docker(conteudo: &str) -> BTreeMap<String, usize> {
    let mut niveis = BTreeMap::new();
    for linha in conteudo.lines() {
        let limpa = remover_ansi(linha);
        // Varre todos os tokens whitespace-delimited em busca de um nível
        // conhecido. Usamos `any` que para no primeiro match (1 nível por
        // linha, mesmo que a linha contenha múltiplas ocorrências).
        if let Some(nivel) = limpa
            .split_whitespace()
            .find(|token| token_eh_nivel(token))
        {
            *niveis.entry(extrair_nivel(nivel)).or_insert(0) += 1;
        }
    }
    niveis
}

/// NÚCLEO PURO: categoriza cada linha de log pelo nível detectado.
/// Devolve um mapa de nível → lista de linhas (já sem códigos ANSI).
/// Normaliza tokens com `|`, `:` etc. nas pontas para capturar formatos
/// Loguru e uvicorn que antes eram ignorados.
pub(crate) fn categorizar_por_nivel(conteudo: &str) -> BTreeMap<String, Vec<String>> {
    let mut grupos: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for linha in conteudo.lines() {
        let limpa = remover_ansi(linha);
        if let Some(nivel) = limpa
            .split_whitespace()
            .find(|token| token_eh_nivel(token))
        {
            grupos.entry(extrair_nivel(nivel)).or_default().push(limpa);
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

/// NÚCLEO PURO: detecta o tipo de aplicação responsável por uma linha de log
/// analisando o formato da linha. Usa heurísticas de padrão textual em vez de
/// posição fixa, porque as linhas vêm de `docker logs` (saída crua da app).
pub(crate) fn detectar_app(linha: &str) -> AppType {
    let t = linha.trim();
    if t.is_empty() {
        return AppType::Outros;
    }

    // Uvicorn: linha começa com nível conhecido seguido de ":" (logs da
    // aplicação) ou " " (access log: "INFO   3.215...").
    // Let chain (edition 2024): combina `let Some` com `&&` para evitar
    // `if` aninhado (lint `collapsible_if` do clippy).
    if let Some(rest) = t.strip_prefix("INFO")
        .or_else(|| t.strip_prefix("WARNING"))
        .or_else(|| t.strip_prefix("ERROR"))
        .or_else(|| t.strip_prefix("CRITICAL"))
        .or_else(|| t.strip_prefix("DEBUG"))
        .or_else(|| t.strip_prefix("TRACE"))
        && (rest.starts_with(':') || rest.starts_with(' '))
    {
        return AppType::Uvicorn;
    }

    // Elefante legado: começa com "["
    if t.starts_with('[') {
        return AppType::Elefante;
    }

    // Data prefixada: tenta detectar "YYYY-MM-DD" no início (verifica bytes).
    let bytes = t.as_bytes();
    if bytes.len() >= 19
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[..4].iter().all(|b| b.is_ascii_digit())
    {
        // Caractere após "YYYY-MM-DD HH:mm:ss" (posição 19 = sub-segundo)
        match bytes[19] {
            b'.' => {
                // Loguru: "YYYY-MM-DD HH:mm:ss.SSS | LEVEL | ..."
                return AppType::Elefante;
            }
            b',' if t.contains(" | ") => {
                // "," + " | " = Elefante (Loguru pode usar vírgula como
                // separador de sub-segundo em alguns locales); "," sem " | "
                // é supervisord ou outro formato → Outros.
                return AppType::Elefante;
            }
            _ => {}
        }
    }

    // " | " sem data = elefante (raro, mas possível se a linha for curta)
    if t.contains(" | ") {
        return AppType::Elefante;
    }

    AppType::Outros
}

/// NÚCLEO PURO: agrupa linhas por tipo de app e, dentro de cada app, por nível
/// de log. Recebe as linhas já sem ANSI (como vêm do banco). Devolve um mapa
/// AppType → (nível → [linhas]).
pub(crate) fn analisar_apps(linhas: &[String]) -> BTreeMap<AppType, BTreeMap<String, Vec<String>>> {
    let mut resultado: BTreeMap<AppType, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for linha in linhas {
        let app = detectar_app(linha);
        // Detecta o nível na linha: normaliza tokens com `|`, `:`, etc.
        // nas pontas (Loguru, uvicorn, etc.) antes de comparar.
        let nivel = linha
            .split_whitespace()
            .find(|token| token_eh_nivel(token))
            .map(extrair_nivel)
            .unwrap_or_else(|| "UNKNOWN".to_string());
        resultado
            .entry(app)
            .or_default()
            .entry(nivel)
            .or_default()
            .push(linha.clone());
    }
    resultado
}

// --- Parse de linhas Loguru (Elefante) --------------------------------------

/// Campos extraídos de uma linha de log no formato Loguru/Elefante:
///
/// ```text
/// YYYY-MM-DD HH:mm:ss.SSS |LEVEL     |module:func:line - [tenant] METHOD STATUS /path  duration [IP] [UA]
/// ```
#[derive(Debug, Clone)]
pub(crate) struct LoguruEntry {
    pub(crate) timestamp: String,
    pub(crate) level: String,
    pub(crate) modulo: String,
    pub(crate) funcao: String,
    pub(crate) linha_numero: u32,
    pub(crate) tenant: String,
    pub(crate) metodo: String,
    pub(crate) status: u16,
    pub(crate) path: String,
    pub(crate) duracao_seg: f64,
    pub(crate) client_ip: String,
    pub(crate) user_agent: String,
}

/// Tenta parsear uma linha como Loguru/Elefante. Retorna `None` se a linha
/// não casa com o formato esperado OU não é do tipo `AppType::Elefante`.
pub(crate) fn parse_loguru_line(linha: &str) -> Option<LoguruEntry> {
    let t = linha.trim();
    if detectar_app(t) != AppType::Elefante {
        return None;
    }

    // Divide pelos pipes: "timestamp | level | [correlation_id] | mensagem"
    // O campo correlation_id é opcional (` - ` quando vazio). A mensagem está
    // na primeira parte depois do level que contenha ":" (módulo:função:linha).
    let partes: Vec<&str> = t.split('|').collect();
    let ts = partes.first()?.trim();
    let level = partes.get(1)?.trim();
    let msg_idx = (2..partes.len()).find(|&i| partes[i].contains(':')).unwrap_or(2);
    let msg = partes.get(msg_idx)?.trim();

    // Mensagem: "module:func:line - [tenant] METHOD STATUS /path  duration [IP] [UA]"
    let (module_part, rest) = msg.split_once(" - ")?;
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.len() < 7 {
        return None;
    }

    // Module: "server.py:server:http_request:112" ou "app.py:main:42"
    let mod_parts: Vec<&str> = module_part.split(':').collect();
    let linha_numero: u32 = mod_parts.last()?.parse().ok()?;
    let funcao = mod_parts.get(mod_parts.len().saturating_sub(2))?.to_string();
    let modulo = if mod_parts.len() > 2 {
        mod_parts[..mod_parts.len().saturating_sub(2)].join(":")
    } else {
        mod_parts[0].to_string()
    };

    // tokens: [tenant] METHOD STATUS path  duration [IP] [UA]
    let tenant = tokens[0].trim_matches(|c| c == '[' || c == ']').to_string();
    let metodo = tokens[1].to_string();
    let status: u16 = tokens[2].parse().ok()?;
    let path = tokens[3].to_string();
    let duracao_seg: f64 = tokens[4].trim_end_matches('s').parse().ok()?;
    let client_ip = tokens[5].trim_matches(|c| c == '[' || c == ']').to_string();
    // tokens[6..] pode ser um user agent com espaços (ex.: "Go-http-client/1.1"
    // sem espaços, mas em outros casos pode ter), juntamos tudo.
    let user_agent = tokens[6..]
        .iter()
        .map(|s| s.trim_matches(|c| c == '[' || c == ']'))
        .collect::<Vec<_>>()
        .join(" ");

    Some(LoguruEntry {
        timestamp: ts.to_string(),
        level: level.to_string(),
        modulo,
        funcao,
        linha_numero,
        tenant,
        metodo,
        status,
        path,
        duracao_seg,
        client_ip,
        user_agent,
    })
}

/// Formata uma entrada Loguru para exibição numa única linha, destacando
/// os campos mais relevantes e encurtando os menos importantes.
pub(crate) fn format_loguru_entry(e: &LoguruEntry) -> String {
    let ts = if e.timestamp.len() >= 19 {
        &e.timestamp[..19]
    } else {
        &e.timestamp
    };
    format!(
        "{} {} {} {} {} {} {}:{} {} {} {:.3}s {}",
        ts,
        e.level,
        e.tenant,
        e.metodo,
        e.status,
        e.path,
        e.modulo,
        e.funcao,
        e.linha_numero,
        e.client_ip,
        e.duracao_seg,
        e.user_agent,
    )
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

    // --- detectar_app -----------------------------------------------------------

    #[test]
    fn detecta_uvicorn_info() {
        assert_eq!(detectar_app("INFO:     Application startup complete."), AppType::Uvicorn);
    }

    #[test]
    fn detecta_uvicorn_warning() {
        assert_eq!(detectar_app("WARNING:  Something is slow."), AppType::Uvicorn);
    }

    #[test]
    fn detecta_uvicorn_error() {
        assert_eq!(detectar_app("ERROR:    Connection refused."), AppType::Uvicorn);
    }

    #[test]
    fn detecta_uvicorn_access_log() {
        let linha = "       INFO   3.215.127.148:32777 - \"POST /ttc/est HTTP/1.1\" 200";
        assert_eq!(detectar_app(linha), AppType::Uvicorn);
    }

    #[test]
    fn detecta_elefante_loguru_com_ponto() {
        let linha = "2025-07-03 09:15:22.123 | INFO     | main:42 - mensagem";
        assert_eq!(detectar_app(linha), AppType::Elefante);
    }

    #[test]
    fn detecta_elefante_loguru_com_correlation_id() {
        let linha = "2025-07-03 09:15:22.123 | INFO | abc-123 | main:42 - msg";
        assert_eq!(detectar_app(linha), AppType::Elefante);
    }

    #[test]
    fn detecta_elefante_legacy_colchete() {
        let linha = "[ 2025-07-03 09:15:22,123 ] {main:42} INFO - mensagem";
        assert_eq!(detectar_app(linha), AppType::Elefante);
    }

    #[test]
    fn detecta_logging_padrao_vira_outros() {
        let linha = "2025-07-03 09:15:22,123 - INFO - mensagem qualquer";
        assert_eq!(detectar_app(linha), AppType::Outros);
    }

    #[test]
    fn detecta_elefante_loguru_com_virgula() {
        let linha = "2025-07-03 09:15:22,123 | INFO | main:42 - msg";
        assert_eq!(detectar_app(linha), AppType::Elefante);
    }

    #[test]
    fn detecta_outros_para_linha_vazia() {
        assert_eq!(detectar_app(""), AppType::Outros);
        assert_eq!(detectar_app("   "), AppType::Outros);
    }

    #[test]
    fn detecta_outros_para_texto_livre() {
        assert_eq!(detectar_app("qualquer coisa sem formato conhecido"), AppType::Outros);
        assert_eq!(detectar_app("2026-07-03  INFO dev_web: server starting"), AppType::Outros);
    }

    // --- analisar_apps ----------------------------------------------------------

    #[test]
    fn analisa_apps_misturados() {
        let linhas = vec![
            "INFO:     Uvicorn started.".to_string(),
            "WARNING:  Slow request.".to_string(),
            "2025-07-03 09:15:22.123 | INFO | main:42 - loguru line".to_string(),
            "2025-07-03 09:15:22,123 - INFO - standard log".to_string(),
            "[ 2025-07-03 09:15:22,123 ] {mod:1} WARN - legacy".to_string(),
        ];
        let resultado = analisar_apps(&linhas);
        assert_eq!(resultado.len(), 3); // Uvicorn, Elefante, Outros
        assert!(resultado.contains_key(&AppType::Uvicorn));
        assert!(resultado.contains_key(&AppType::Elefante));
        assert!(resultado.contains_key(&AppType::Outros));
    }

    #[test]
    fn analisa_apps_linhas_sem_nivel_viram_unknown() {
        let linhas = vec![
            "INFO:     normal uvicorn".to_string(),
            "texto livre sem nivel".to_string(),
        ];
        let resultado = analisar_apps(&linhas);
        let uvicorn = resultado.get(&AppType::Uvicorn).unwrap();
        assert!(uvicorn.contains_key("INFO"));
        let outros = resultado.get(&AppType::Outros).unwrap();
        assert!(outros.contains_key("UNKNOWN"));
    }

    // --- parse_loguru_line -------------------------------------------------------

    #[test]
    fn parse_loguru_linha_completa() {
        let linha = "2026-07-07 14:09:08.185 |INFO     |server.py:server:http_request:112 - [tlantic] POST 200 /ttc/est  0.00485s [3.215.127.148] [Go-http-client/1.1]";
        let e = parse_loguru_line(linha).unwrap();
        assert_eq!(e.timestamp, "2026-07-07 14:09:08.185");
        assert_eq!(e.level, "INFO");
        assert_eq!(e.modulo, "server.py:server");
        assert_eq!(e.funcao, "http_request");
        assert_eq!(e.linha_numero, 112);
        assert_eq!(e.tenant, "tlantic");
        assert_eq!(e.metodo, "POST");
        assert_eq!(e.status, 200);
        assert_eq!(e.path, "/ttc/est");
        assert!((e.duracao_seg - 0.00485).abs() < 1e-6);
        assert_eq!(e.client_ip, "3.215.127.148");
        assert_eq!(e.user_agent, "Go-http-client/1.1");
    }

    #[test]
    fn parse_loguru_linha_simples() {
        let linha = "2026-07-07 14:09:08.185 |INFO     |app:main:42 - [default] GET 200 /health  0.00123s [127.0.0.1] [test-agent]";
        let e = parse_loguru_line(linha).unwrap();
        assert_eq!(e.modulo, "app");
        assert_eq!(e.funcao, "main");
        assert_eq!(e.linha_numero, 42);
        assert_eq!(e.metodo, "GET");
        assert_eq!(e.status, 200);
        assert_eq!(e.path, "/health");
    }

    #[test]
    fn parse_loguru_rejeita_linha_nao_elefante() {
        assert!(parse_loguru_line("INFO:     uvicorn line").is_none());
        assert!(parse_loguru_line("").is_none());
        assert!(parse_loguru_line("2026-07-07 14:09:08,186 DEBG 'stdout output:").is_none());
    }

    #[test]
    fn detecta_elefante_loguru_com_pipe_dash_pipe() {
        let linha = "2026-07-07 17:10:08.651 |INFO    | - |droide.py:controllers.droide:executar:226 - Dróide encerrou.";
        assert_eq!(detectar_app(linha), AppType::Elefante);
    }

    #[test]
    fn parse_loguru_linha_com_pipe_dash_pipe_rejeita_por_msg_livre() {
        let linha = "2026-07-07 17:10:08.651 |INFO    | - |droide.py:controllers.droide:executar:226 - Dróide encerrou.";
        assert!(parse_loguru_line(linha).is_none());
    }

    #[test]
    fn parse_loguru_linha_com_correlation_id_e_request() {
        let linha = "2026-07-07 14:09:08.185 |INFO     | abc-123 |server.py:server:http_request:112 - [tlantic] POST 200 /ttc/est  0.00485s [3.215.127.148] [Go-http-client/1.1]";
        let e = parse_loguru_line(linha).unwrap();
        assert_eq!(e.timestamp, "2026-07-07 14:09:08.185");
        assert_eq!(e.level, "INFO");
        assert_eq!(e.modulo, "server.py:server");
        assert_eq!(e.metodo, "POST");
        assert_eq!(e.path, "/ttc/est");
    }

    #[test]
    fn contar_niveis_docker_qa_prezzo() {
        let conteudo = "\
2026-07-07 17:10:08.651 |INFO    | - |droide.py:controllers.droide:executar:226 - encerrou
2026-07-07 17:10:13.967 |DEBUG   | - |elefante.py:elefante:<module>:62 - debug msg";
        let niveis = contar_niveis_docker(conteudo);
        assert_eq!(niveis.get("INFO"), Some(&1));
        assert_eq!(niveis.get("DEBUG"), Some(&1));
        assert_eq!(niveis.len(), 2);
    }

    #[test]
    fn format_loguru_produz_string_compacta() {
        let linha = "2026-07-07 14:09:08.185 |INFO     |server:http_request:112 - [tlantic] POST 200 /ttc/est  0.00485s [3.215.127.148] [Go-http-client/1.1]";
        let e = parse_loguru_line(linha).unwrap();
        let saida = format_loguru_entry(&e);
        assert!(saida.starts_with("2026-07-07 14:09:08"));
        assert!(saida.contains("POST"));
        assert!(saida.contains("200"));
        assert!(saida.contains("/ttc/est"));
        assert!(saida.contains("http_request"));
        assert!(saida.contains("server:http_request")); // modulo:funcao
        assert!(saida.contains("0.005s")); // .00485 rounded to 0.005
    }
}
