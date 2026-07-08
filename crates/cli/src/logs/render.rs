// RENDERIZAÇÃO: monta o texto colorido a partir das contagens,
// sem saber de onde elas vieram.

use std::collections::BTreeMap;

// Trait de extensão do `owo-colors`: ao importá-la, todo tipo que implementa
// `Display` ganha métodos como `.red()`, `.bold()`, `.dimmed()`.
// docs: https://docs.rs/owo-colors/latest/owo_colors/trait.OwoColorize.html
use owo_colors::OwoColorize;
use rusqlite::Connection;

use nucleo::core::Contagens;

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

/// Lê as contagens acumuladas do banco e formata para exibição.
pub(crate) fn exibir_estatisticas(conn: &Connection) -> Result<String, Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT container_name, level, SUM(count) as total
         FROM log_counts
         GROUP BY container_name, level
         ORDER BY container_name, level",
    )?;

    // Mapa aninhado: container -> (nível -> total). O `BTreeMap` externo dá
    // ordem alfabética por container; o interno, por nível.
    let mut dados: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    // O closure de `query_map` roda por linha e pode falhar em cada `row.get`
    // (tipo errado, coluna ausente); por isso ele próprio devolve `Result`,
    // e usamos `?` dentro dele para propagar esse erro por linha.
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Statement.html#method.query_map
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Row.html#method.get
    let linhas = stmt.query_map([], |row| {
        let nome: String = row.get(0)?;
        let nivel: String = row.get(1)?;
        let total: i64 = row.get(2)?;
        Ok((nome, nivel, total as usize))
    })?;

    for linha in linhas {
        // Aqui o `?` é sobre o `Result` de CADA linha (o iterador de
        // `query_map` produz `Result<(...), Error>`), não sobre o closure
        // acima.
        let (nome, nivel, total) = linha?;
        // API `entry`: garante um `BTreeMap` vazio para containers novos
        // antes de inserir o par nível/total.
        // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.entry
        // docs: https://doc.rust-lang.org/std/collections/btree_map/enum.Entry.html#method.or_default
        // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.insert
        dados.entry(nome).or_default().insert(nivel, total);
    }

    // Carrega o status (uptime) de cada container do banco
    let mut stmt2 = conn
        .prepare("SELECT name, uptime FROM containers WHERE uptime IS NOT NULL AND uptime != ''")?;
    let mut status_map: BTreeMap<String, String> = BTreeMap::new();
    // `.flatten()` sobre um iterador de `Result<(String, String), Error>`
    // funciona porque `Result` também implementa `IntoIterator` (0 ou 1
    // item): `Ok(x)` vira um iterador de 1 elemento, `Err(_)` vira vazio —
    // é uma forma mais curta de "ignore os erros e siga com os `Ok`".
    // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.flatten
    // docs: https://doc.rust-lang.org/std/result/enum.Result.html
    // docs: https://doc.rust-lang.org/std/iter/trait.IntoIterator.html
    for row in stmt2
        .query_map([], |r| {
            let n: String = r.get(0)?;
            let s: String = r.get(1)?;
            Ok((n, s))
        })?
        .flatten()
    {
        status_map.insert(row.0, row.1);
    }

    let mut saida = String::new();
    for (nome, niveis) in &dados {
        let status = status_map.get(nome).map(|s| s.as_str());
        saida.push_str(&renderizar_container(nome, status, niveis));
    }
    Ok(saida)
}
