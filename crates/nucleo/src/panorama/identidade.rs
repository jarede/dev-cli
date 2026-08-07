// Módulo `identidade`: unificação da autoria dos commits de um repositório.
//
// A API de contribuidores do GitLab chaveia por par (nome, e-mail) — e a
// mesma pessoa aparece várias vezes porque `user.email` muda: conta antiga,
// e-mail corporativo diferente, e-mail de privacidade do GitHub etc. Sem
// unificar, "autoria por repositório" vira ruído — um humano vira N entradas.
//
// Este módulo é PURAMENTE de cálculo (texto -> valores), sem I/O: recebe os
// pares crus (nome, e-mail, commits) e devolve os `Autor` do contrato
// (`panorama::snapshot`) já unificados e ordenados — o coletor GitLab apenas
// orquestra a chamada.

use std::collections::{BTreeMap, BTreeSet};

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use crate::panorama::snapshot::Autor;

/// Um par (nome, e-mail) cru vindo da API de contribuidores, com a contagem
/// de commits exata daquela combinação.
pub struct Contribuidor {
    pub nome: String,
    pub email: String,
    pub commits: u64,
}

/// Um apelido configurado manualmente: a chave do `BTreeMap` é o
/// identificador canônico da pessoa; `nome` é o nome exibido (opcional) e
/// `padroes` são substrings que casam o alvo `"{nome} {email}"`.
///
/// Configuração manual vence a heurística: é a saída de emergência quando a
/// local-part do e-mail não identifica a pessoa (ex.: "dev@exemplo.interno"
/// é compartilhado).
pub struct Apelido {
    pub nome: Option<String>,
    pub padroes: Vec<String>,
}

/// Verdadeiro apenas para bots de verdade, nunca para e-mail de privacidade.
pub fn eh_bot(nome: &str, email: &str) -> bool {
    // ARMADILHA REGISTRADA — não "simplificar" este filtro:
    // `<id>+<usuario>@users.noreply.github.com` é o e-mail de privacidade de
    // UMA PESSOA REAL. Uma versão anterior casava a substring "noreply" e
    // descartou 592 commits de um autor humano. Aqui casamos APENAS:
    //   1. a substring literal "[bot]" no nome;
    //   2. nomes conhecidos NO INÍCIO do campo nome;
    //   3. local-part "bot@": `bot@` no começo do e-mail ou precedido de
    //      `.`, `+`, `_` ou `-`. O delimitador é o que faz `abbot@` e
    //      `robotoiro@` NÃO casarem — ver testes.
    const NOMES_BOT: [&str; 5] = [
        "dependabot",
        "renovate",
        "copilot",
        "semantic-release",
        "gitlab-bot",
    ];
    const DELIMITADORES_BOT: [&str; 4] = [".bot@", "+bot@", "_bot@", "-bot@"];

    let nome_min = nome.to_lowercase();
    let email_min = email.to_lowercase();

    nome_min.contains("[bot]")
        || NOMES_BOT
            .iter()
            .any(|prefixo| nome_min.starts_with(prefixo))
        || email_min.starts_with("bot@")
        || DELIMITADORES_BOT
            .iter()
            .any(|marca| email_min.contains(marca))
}

/// Remove acentos via normalização Unicode NFD (decomposição canônica):
/// a letra "ã" é decomposta em `a` + uma marca combinante (U+0303). NFD
/// isola a marca, e `is_combining_mark` a descarta — sobra a letra nua.
/// Assim "João" e "Joao" produzem a mesma string, e a mesma pessoa não é
/// separada pela grafia com/sem acento.
/// docs: https://www.unicode.org/reports/tr15/
fn remover_acento(texto: &str) -> String {
    texto
        .nfd()
        .filter(|caractere| !is_combining_mark(*caractere))
        .collect()
}

/// Reduz (nome, email) ao identificador canônico da pessoa por trás deles.
///
/// Ordem das tentativas:
///   1. Se algum `padrao` de algum apelido for substring do alvo
///      `"{nome sem acento, minúsculo} {email minúsculo}"`, devolve a chave
///      desse apelido — configuração manual vence a heurística.
///   2. Senão, a local-part do e-mail (parte antes do `@`), sem acento e
///      mantendo apenas `[a-z.]`.
///   3. Se isso ficar vazio, o nome sem acento em minúsculas.
///   4. Se ainda ficar vazio, `"desconhecido"`.
pub fn chave_canonica(nome: &str, email: &str, apelidos: &BTreeMap<String, Apelido>) -> String {
    let nome_limpo = remover_acento(nome).to_lowercase();
    let email_min = email.to_lowercase();
    // O alvo une os dois campos porque o apelido pode casar por nome, por
    // e-mail ou pela combinação deles — é o texto sobre o qual se procura.
    let alvo = format!("{nome_limpo} {email_min}");

    // 1. Apelido manual: se qualquer padrão é substring do alvo, a chave é
    // a própria chave do apelido (o rótulo escolhido pelo humano).
    for (chave, apelido) in apelidos {
        if apelido.padroes.iter().any(|padrao| alvo.contains(padrao)) {
            return chave.clone();
        }
    }

    // 2. Local-part do e-mail, sem acento e mantendo só `[a-z.]`. Ex.:
    // "12345+gabriel@users.noreply.github.com" vira "gabriel" — unificando
    // com os demais e-mails do autor. Por isso a ARMADILHA do `eh_bot` é
    // crítica: esses usuários são PESSOAS.
    let local = remover_acento(email_min.split('@').next().unwrap_or_default());
    let local_part: String = local
        .chars()
        .filter(|caractere| caractere.is_ascii_lowercase() || *caractere == '.')
        .collect();
    if !local_part.is_empty() {
        return local_part;
    }

    // 3. E-mail sem local-part utilizável (ex.: "só-acentos"): cai no nome.
    // 4. Último recurso, para nunca devolver uma string vazia como chave.
    if !nome_limpo.is_empty() {
        nome_limpo
    } else {
        "desconhecido".to_string()
    }
}

/// Junta as identidades da mesma pessoa, remove bots, ordena e calcula os
/// percentuais — o `Autor` do contrato do snapshot.
pub fn unificar_autores(
    contribuidores: &[Contribuidor],
    apelidos: &BTreeMap<String, Apelido>,
) -> Vec<Autor> {
    // Acumulador por chave canônica: commits somados, e-mails distintos e o
    // nome mais longo visto (o mais informativo — ver teste do apelido).
    #[derive(Default)]
    struct Acumulado {
        commits: u64,
        emails: BTreeSet<String>,
        nome_mais_longo: String,
    }

    // Bots são descartados ANTES de agrupar: se filtrássemos depois, os
    // commits deles entrariam no `total` e os percentuais sairiam errados.
    let mut por_chave: BTreeMap<String, Acumulado> = BTreeMap::new();
    for contribuidor in contribuidores {
        if eh_bot(&contribuidor.nome, &contribuidor.email) {
            continue;
        }
        // API `entry`: sem busca manual seguida de insert — ou cria o
        // acumulado com o padrão (`or_default`) ou devolve o existente para
        // mutar no lugar.
        let acumulado = por_chave
            .entry(chave_canonica(
                &contribuidor.nome,
                &contribuidor.email,
                apelidos,
            ))
            .or_default();
        acumulado.commits += contribuidor.commits;
        // `BTreeSet` já ordena lexicograficamente e elimina duplicados.
        acumulado.emails.insert(contribuidor.email.clone());
        // Só troca o nome se o novo for mais longo (mais informativo).
        if contribuidor.nome.len() > acumulado.nome_mais_longo.len() {
            acumulado.nome_mais_longo.clone_from(&contribuidor.nome);
        }
    }

    let total: u64 = por_chave.values().map(|acumulado| acumulado.commits).sum();

    let mut autores: Vec<Autor> = por_chave
        .into_iter()
        .map(|(chave, acumulado)| {
            // Nome exibido: o do apelido se houver; senão o mais longo. O
            // `unwrap_or` é seguro: cai no nome coletado quando não há
            // apelido ou quando o apelido não define `nome`.
            let nome = apelidos
                .get(&chave)
                .and_then(|apelido| apelido.nome.clone())
                .unwrap_or(acumulado.nome_mais_longo);
            Autor {
                nome,
                commits: acumulado.commits,
                emails: acumulado.emails.into_iter().collect(),
                percentual: percentual(acumulado.commits, total),
            }
        })
        .collect();

    // Ordenação determinística: commits decrescente; empate desempata pelo
    // nome (ascendente) — mesma entrada, mesma saída, sempre.
    autores.sort_by(|a, b| b.commits.cmp(&a.commits).then_with(|| a.nome.cmp(&b.nome)));
    autores
}

/// Arredondamento de `100 * commits / total` (metade para cima).
///
/// Total zero — nada sobreviveu ao filtro de bots — devolve 0 em vez de
/// dividir por zero. `saturating_mul`/`saturating_add` evitam estouro de
/// `u64` com contagens absurdas vindas de entrada; `min(100)` capa a saída
/// no percentual máximo plausível.
fn percentual(commits: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }
    let arredondado = commits.saturating_mul(100).saturating_add(total / 2) / total;
    arredondado.min(100) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apelidos_vazios() -> BTreeMap<String, Apelido> {
        BTreeMap::new()
    }

    fn contribuidor(nome: &str, email: &str, commits: u64) -> Contribuidor {
        Contribuidor {
            nome: nome.to_string(),
            email: email.to_string(),
            commits,
        }
    }

    /// ARMADILHA: `<id>+<usuario>@users.noreply.github.com` é o e-mail de
    /// privacidade de UMA PESSOA REAL, não de bot. Quem "simplificar" este
    /// filtro casando a substring "noreply" some com os commits desse autor —
    /// aconteceu uma vez (592 commits descartados) e não deve acontecer de
    /// novo.
    #[test]
    fn email_de_privacidade_nao_e_bot() {
        assert!(!eh_bot("Gabriel", "12345+gabriel@users.noreply.github.com"));
    }

    #[test]
    fn reconhece_bots_de_verdade() {
        assert!(eh_bot(
            "dependabot[bot]",
            "49699333+dependabot[bot]@users.noreply.github.com",
        ));
        assert!(eh_bot(
            "renovate[bot]",
            "29139614+renovate[bot]@users.noreply.github.com",
        ));
        assert!(eh_bot("copilot", "copilot@exemplo.interno"));
        assert!(eh_bot("semantic-release", "release@exemplo.interno"));
        assert!(eh_bot("Integração", "bot@exemplo.interno"));
        assert!(eh_bot("Integração", "ci.bot@exemplo.interno"));
    }

    #[test]
    fn humanos_parecidos_com_bot_nao_sao_bot() {
        // "robotoiro@" e "abbot@" contêm a substring "bot@", mas sem o
        // delimitador exigido (`.`, `+`, `_`, `-` ou início do e-mail) —
        // logo não casam.
        assert!(!eh_bot("Robotoiro", "robotoiro@exemplo.interno"));
        assert!(!eh_bot("Abbot", "abbot@exemplo.interno"));
        // Substring "bot" apenas no NOME (sem ser local-part `bot@`) também
        // não conta: "Roberto Bottini" é gente real.
        assert!(!eh_bot("Roberto Bottini", "r.bottini@exemplo.interno"));
    }

    /// Três e-mails da mesma pessoa (mesma local-part) somam num autor só,
    /// com os três e-mails na lista, ordenados lexicograficamente.
    #[test]
    fn unifica_tres_emails_da_mesma_pessoa() {
        let contribuidores = [
            contribuidor("J. Silva", "jsilva@exemplo.interno", 2),
            contribuidor("Jarede F. Silva", "jsilva@exemplo.interno2", 5),
            contribuidor("J. F. S.", "jsilva@outro.interno", 3),
        ];
        let autores = unificar_autores(&contribuidores, &apelidos_vazios());
        assert_eq!(autores.len(), 1);
        assert_eq!(autores[0].commits, 10);
        assert_eq!(
            autores[0].emails,
            vec![
                "jsilva@exemplo.interno".to_string(),
                "jsilva@exemplo.interno2".to_string(),
                "jsilva@outro.interno".to_string(),
            ],
        );
    }

    /// Acento não separa: "João" e "Joao" são a mesma pessoa. A normalização
    /// NFD cai na chave canônica — e até local-part com acento normaliza.
    #[test]
    fn acento_nao_separa_a_mesma_pessoa() {
        let vazios = apelidos_vazios();
        assert_eq!(
            chave_canonica("João", "joao@exemplo.interno", &vazios),
            chave_canonica("Joao", "joao@exemplo.interno", &vazios),
        );
        // Local-part acentuada também normaliza: "joão" -> "joao".
        assert_eq!(
            chave_canonica("Anônimo", "joão@exemplo.interno", &vazios),
            "joao".to_string(),
        );

        let contribuidores = [
            contribuidor("João Pedro", "joao@exemplo.interno", 4),
            contribuidor("Joao", "joao@exemplo.interno", 6),
        ];
        let autores = unificar_autores(&contribuidores, &vazios);
        assert_eq!(autores.len(), 1);
        assert_eq!(autores[0].commits, 10);
    }

    /// Apelido com `nome` definido vence os nomes vindos do Git — mesmo
    /// quando o nome do Git é mais longo.
    #[test]
    fn apelido_vence_nomes_do_git() {
        let apelidos = BTreeMap::from([(
            "jsilva".to_string(),
            Apelido {
                nome: Some("Jarede F. Silva".to_string()),
                padroes: vec!["jsilva".to_string()],
            },
        )]);
        let contribuidores = [
            contribuidor("Jose Silva Jarede Ferreira", "jsilva@exemplo.interno", 4),
            contribuidor("J. F. S.", "jsilva@exemplo.interno2", 2),
        ];
        let autores = unificar_autores(&contribuidores, &apelidos);
        assert_eq!(autores.len(), 1);
        assert_eq!(autores[0].nome, "Jarede F. Silva");
        assert_eq!(autores[0].commits, 6);
    }

    /// Sem apelido, o nome mais longo vence: "J. Silva" e "Jarede F. Silva"
    /// são a mesma pessoa; o segundo é mais informativo.
    #[test]
    fn sem_apelido_vence_nome_mais_longo() {
        let contribuidores = [
            contribuidor("J. Silva", "jsilva@exemplo.interno", 4),
            contribuidor("Jarede F. Silva", "jsilva@exemplo.interno", 6),
        ];
        let autores = unificar_autores(&contribuidores, &apelidos_vazios());
        assert_eq!(autores.len(), 1);
        assert_eq!(autores[0].nome, "Jarede F. Silva");
    }

    /// Percentuais de um caso conhecido (total 120): 100 -> 83, 15 -> 13,
    /// 5 -> 4. Lista vazia devolve vazio sem panic.
    #[test]
    fn percentuais_batem_e_lista_vazia_nao_panica() {
        let contribuidores = [
            contribuidor("Alice A.", "a@exemplo.interno", 100),
            contribuidor("Bruno B.", "b@exemplo.interno", 15),
            contribuidor("Carla C.", "c@exemplo.interno", 5),
        ];
        let autores = unificar_autores(&contribuidores, &apelidos_vazios());
        let resumo: Vec<(&str, u8)> = autores
            .iter()
            .map(|autor| (autor.nome.as_str(), autor.percentual))
            .collect();
        assert_eq!(
            resumo,
            vec![("Alice A.", 83), ("Bruno B.", 13), ("Carla C.", 4)],
        );

        assert!(unificar_autores(&[], &apelidos_vazios()).is_empty());
    }

    /// Todos os contribuidores sendo bots devolve lista vazia — e, como o
    /// total fica zero, `percentual` devolve 0 sem divisão por zero.
    #[test]
    fn so_bots_devolve_vazio() {
        let contribuidores = [
            contribuidor(
                "dependabot[bot]",
                "49699333+dependabot[bot]@users.noreply.github.com",
                50,
            ),
            contribuidor(
                "renovate[bot]",
                "29139614+renovate[bot]@users.noreply.github.com",
                30,
            ),
        ];
        let autores = unificar_autores(&contribuidores, &apelidos_vazios());
        assert!(autores.is_empty());
    }
}
