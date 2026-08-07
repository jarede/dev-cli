// MÓDULO PURO: parse do log do proxy reverso de borda (nginx). O proxy
// registra toda requisição HTTP numa linha; daqui saem o volume por
// aplicação (vhost) e as máquinas de origem. Esta issue é SÓ o parse —
// obter o log (docker logs) fica em outra issue.
//
// Por que "puro"? Sem I/O nenhum: recebe `&str`, devolve dados. Isso torna
// o módulo 100% testável com strings inline e permite reusá-lo com qualquer
// fonte de log (local ou remota) sem mudar nada aqui.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use regex::Regex;

use crate::panorama::snapshot::{DiaRequisicoes, RotaContagem, VHost};

/// Abreviações de mês que o nginx grava no timestamp — sempre em inglês.
/// O índice no array (0 a 11) + 1 é o número do mês (Jan = 01). Um array
/// de tamanho fixo é mais barato e mais à prova de erro que um `match` de
/// doze braços.
const MESES: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Regex da limpeza ANSI, compilada UMA única vez. `OnceLock` (std, sem
/// crates extras) garante que a compilação — cara — aconteça na primeira
/// chamada e seja reaproveitada nas milhões de linhas seguintes. Padrão:
/// ESC seguido de `[`, códigos `0-9`/`;` e `m` (ex.: "\x1b[0;33;1m").
static LIMPEZA_ANSI: OnceLock<Regex> = OnceLock::new();

/// Uma requisição já parseada e normalizada.
///
/// `data` vem derivada do timestamp do nginx; `rota` vem sem query string
/// (ver `extrair_rota`); `status` é numérico.
#[derive(Debug, Clone, PartialEq)]
pub struct Requisicao {
    pub vhost: String,
    pub ip: String,
    /// "YYYY-MM-DD", derivada do timestamp da linha.
    pub data: String,
    /// Sem query string: "/pedidos?id=9" vira "/pedidos".
    pub rota: String,
    pub status: u16,
}

/// Faz o parse de uma linha do log. Devolve `None` para linha malformada —
/// e isso é o comportamento NORMAL, não um erro: num log de borda entram
/// erros do próprio nginx, linhas truncadas por rotação e todo tipo de
/// ruído. `Option` em vez de `Result` comunica exatamente isso: "não é uma
/// requisição, siga em frente".
pub fn analisar_linha(linha: &str) -> Option<Requisicao> {
    let limpa = limpar_ansi(linha);
    let sem_prefixo = remover_prefixo_docker(&limpa).trim();
    let mut tokens = sem_prefixo.split_whitespace();

    // Campos posicionais do início: vhost, IP do cliente e os dois "-" de
    // identidade (formato combined do nginx). Não validamos o conteúdo dos
    // "-": se faltarem tokens, os `?` a seguir devolvem `None` sozinhos.
    let vhost = tokens.next()?.to_string();
    let ip = tokens.next()?.to_string();
    tokens.next()?;
    tokens.next()?;

    // Timestamp: "[07/Aug/2026:14:22:31 +0000]". O fuso horário vem num
    // token separado (há um espaço dentro dos colchetes), então juntamos
    // tokens até fechar o ']'.
    let primeiro_ts = tokens.next()?;
    if !primeiro_ts.starts_with('[') {
        return None;
    }
    let mut bloco_ts = primeiro_ts.to_string();
    while !bloco_ts.ends_with(']') {
        let proximo = tokens.next()?;
        bloco_ts.push(' ');
        bloco_ts.push_str(proximo);
    }
    let data = extrair_data(&bloco_ts)?;

    // Bloco entre aspas da requisição: "<método> <rota> <proto>".
    let primeiro_req = tokens.next()?;
    if !primeiro_req.starts_with('"') {
        return None;
    }
    let mut bloco_req = primeiro_req.to_string();
    while !bloco_req.ends_with('"') {
        let proximo = tokens.next()?;
        bloco_req.push(' ');
        bloco_req.push_str(proximo);
    }
    let rota = extrair_rota(&bloco_req)?;

    // O status é o token IMEDIATAMENTE depois do bloco da requisição —
    // ancorar aqui é o que torna o parse imune a aspas no user-agent: o que
    // vem depois do status pode ser qualquer coisa e não interessa.
    let status: u16 = tokens.next()?.parse().ok()?;
    // O campo bytes existe na linha bem formada; sem ele a linha está
    // truncada. O valor em si não entra no snapshot, só a presença conta.
    tokens.next()?;

    Some(Requisicao {
        vhost,
        ip,
        data,
        rota,
        status,
    })
}

/// Extrai a data "YYYY-MM-DD" do bloco de timestamp "[dia/Mês/ano:hh:mm:ss
/// +fuso]". O mês em texto é mapeado para número; mês desconhecido devolve
/// `None` — uma requisição com data impossível não pode entrar na série.
fn extrair_data(bloco: &str) -> Option<String> {
    let interior = bloco.trim_matches(|c| c == '[' || c == ']');
    let mut partes = interior.split('/');
    let dia: u32 = partes.next()?.parse().ok()?;
    let mes_abreviado = partes.next()?;
    // O ano chega grudado na hora: "2026:14:22:31"; interessa só o ano.
    let ano = partes.next()?.get(..4)?;

    if !(1..=31).contains(&dia) {
        return None;
    }
    if ano.len() != 4 || !ano.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // `position()` devolve o índice do mês no array; +1 porque o array é
    // 0-based e janeiro é o mês 1. O `?` propaga "mês desconhecido" como
    // `None`, descartando a linha.
    let mes = MESES.iter().position(|m| *m == mes_abreviado)? + 1;
    Some(format!("{ano}-{mes:02}-{dia:02}"))
}

/// Extrai a rota do bloco entre aspas da requisição, sem a query string.
/// A query identifica UM recurso ("?id=9"), mas o volume agregado é por
/// ROTA ("/pedidos") — dois pedidos de produtos diferentes caem na mesma
/// rota. Bloco truncado no meio devolve `None`.
fn extrair_rota(bloco: &str) -> Option<String> {
    let interior = bloco.trim_matches('"');
    let mut partes = interior.split_whitespace();
    partes.next()?; // método ("GET") — irrelevante para o volume
    let rota_completa = partes.next()?;
    partes.next()?; // protocolo ("HTTP/1.1") — irrelevante
    // Corta no primeiro '?': "?id=9" e "?id=10" são a mesma rota.
    rota_completa.split('?').next().map(str::to_string)
}

/// Acumulador por vhost durante a passada única do fluxo de linhas.
/// Fica privado: é forma de trabalho; o contrato público é `VHost`.
#[derive(Default)]
struct AcumuladorVHost {
    requisicoes: u64,
    erros_4xx: u64,
    erros_5xx: u64,
    /// data "YYYY-MM-DD" -> contagens do dia. BTreeMap: a iteração já sai
    /// em ordem cronológica (no formato ISO, lexicográfica == cronológica).
    dias: BTreeMap<String, DiaRequisicoes>,
    /// BTreeSet: IPs distintos e já em ordem — sem repetição nem sort final.
    maquinas: BTreeSet<String>,
    /// rota -> total. BTreeMap dá o desempate alfabético na finalização.
    rotas: BTreeMap<String, u64>,
}

/// Agrega um fluxo de linhas do log em `VHost` prontos para o snapshot.
///
/// Recebe um iterador e processa linha a linha, agregando em mapas: num
/// log de borda com milhões de linhas por janela, materializar tudo em
/// memória não é uma opção — o custo é O(n) em tempo e O(únicos) em memória,
/// não O(n).
pub fn agregar<'a>(linhas: impl Iterator<Item = &'a str>) -> Vec<VHost> {
    // BTreeMap por vhost: além da agregação, já mantém os vhosts em ordem
    // alfabética, o que torna o resultado determinístico — mesma entrada,
    // mesma saída, qualquer que seja a ordem de chegada das linhas.
    let mut por_vhost: BTreeMap<String, AcumuladorVHost> = BTreeMap::new();

    for linha in linhas {
        let Some(requisicao) = analisar_linha(linha) else {
            // Linha malformada é o esperado num log sujo: ignora e segue.
            // O let-else evita aninhar o corpo inteiro num `if let`.
            continue;
        };

        // API `entry`: pega o acumulador do vhost, criando um vazio se for
        // o primeiro contato — evita "verificar, buscar, senão criar" em
        // três chamadas separadas. `or_default()` = `or_insert_with(Default::default)`.
        let acumulador = por_vhost.entry(requisicao.vhost).or_default();
        acumulador.requisicoes += 1;

        let dia = acumulador.dias.entry(requisicao.data).or_default();
        dia.requisicoes += 1;

        // Faixa de erro: 4xx são erros do cliente, 5xx do servidor — o
        // snapshot separa as duas leituras. Demais status não são erros.
        // `match` com ranges evita dois `if` encadeados.
        match requisicao.status {
            400..=499 => {
                acumulador.erros_4xx += 1;
                dia.erros_4xx += 1;
            }
            500..=599 => {
                acumulador.erros_5xx += 1;
                dia.erros_5xx += 1;
            }
            _ => {}
        }

        acumulador.maquinas.insert(requisicao.ip);
        *acumulador.rotas.entry(requisicao.rota).or_insert(0) += 1;
    }

    let mut resultado: Vec<VHost> = por_vhost
        .into_iter()
        .map(|(vhost, acumulador)| {
            let mut rotas_top: Vec<RotaContagem> = acumulador
                .rotas
                .into_iter()
                .map(|(rota, requisicoes)| RotaContagem { rota, requisicoes })
                .collect();
            // As 20 mais pedidas: contagem decrescente; empate desempata
            // por rota (alfabética), tornando a ordem determinística.
            rotas_top.sort_by(|a, b| {
                b.requisicoes
                    .cmp(&a.requisicoes)
                    .then_with(|| a.rota.cmp(&b.rota))
            });
            rotas_top.truncate(20);

            VHost {
                vhost,
                requisicoes: acumulador.requisicoes,
                erros_4xx: acumulador.erros_4xx,
                erros_5xx: acumulador.erros_5xx,
                // BTreeMap -> Vec: já sai em ordem cronológica de data. O
                // campo `data` do `DiaRequisicoes` não é preenchido pelo
                // `or_default()` (que zera tudo), então copia a chave aqui.
                dias: acumulador
                    .dias
                    .into_iter()
                    .map(|(data, mut dia)| {
                        dia.data = data;
                        dia
                    })
                    .collect(),
                maquinas: acumulador.maquinas.into_iter().collect(),
                rotas_top,
            }
        })
        .collect();

    // Volume decrescente; empate desempata por vhost (determinismo).
    resultado.sort_by(|a, b| {
        b.requisicoes
            .cmp(&a.requisicoes)
            .then_with(|| a.vhost.cmp(&b.vhost))
    });
    resultado
}

/// Remove códigos ANSI de cor que o `docker compose logs` injeta no início
/// e no fim de cada linha. Sem isso o primeiro campo (o vhost) viria
/// poluído pelo escape e jamais casaria.
fn limpar_ansi(linha: &str) -> String {
    let regex = LIMPEZA_ANSI.get_or_init(|| {
        // Padrão literal de constante: a compilação só falharia se o
        // próprio código estivesse errado — `expect` nunca dispara aqui.
        Regex::new(r"\x1b\[[0-9;]*m").expect("regex ANSI constante válida")
    });
    // `replace_all` porque a linha pode ter mais de um código (um no início,
    // outro depois do pipe). `into_owned()` materializa o `Cow`.
    regex.replace_all(linha, "").into_owned()
}

/// Remove o prefixo que o `docker compose logs` cola em cada linha:
/// "<nome-do-container>    | ". A separação pelo primeiro pipe exige
/// cuidado: um pipe legítimo dentro do user-agent não pode ser confundido
/// com o separador — por isso só tratamos o pipe como separador quando a
/// parte da esquerda parece um nome de container (letras, números, ponto,
/// hífen e sublinhado, ex.: "nginx.1").
fn remover_prefixo_docker(linha: &str) -> &str {
    let Some((nome, resto)) = linha.split_once('|') else {
        return linha;
    };
    let nome = nome.trim();
    let parece_nome = !nome.is_empty()
        && nome
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_');
    if parece_nome { resto } else { linha }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Linha bem formada, sem prefixo — base dos demais testes.
    fn linha_valida() -> &'static str {
        r#"app.exemplo.interno 10.1.30.44 - - [07/Aug/2026:14:22:31 +0000] "GET /pedidos?id=9 HTTP/1.1" 200 5120 "-" "Mozilla/5.0 (X11; Linux)" "172.20.0.7:8000""#
    }

    /// Acceptance: linha bem formada produz `Requisicao` com todos os
    /// campos corretos — vhost, IP, data derivada do timestamp, rota SEM
    /// query string e status numérico.
    #[test]
    fn linha_bem_formada_parseia_todos_os_campos() {
        let requisicao = analisar_linha(linha_valida()).expect("linha válida");
        assert_eq!(requisicao.vhost, "app.exemplo.interno");
        assert_eq!(requisicao.ip, "10.1.30.44");
        assert_eq!(requisicao.data, "2026-08-07");
        assert_eq!(requisicao.rota, "/pedidos");
        assert_eq!(requisicao.status, 200);
    }

    /// Acceptance: a linha como sai de `docker compose logs` — código ANSI
    /// de cor + "<nome>    | " — parseia igual à linha limpa.
    #[test]
    fn prefixo_ansi_e_docker_nao_alteram_o_parse() {
        let com_prefixo = format!("\u{1b}[0;33;1mnginx.1     | \u{1b}[0m{}", linha_valida());
        assert_eq!(analisar_linha(&com_prefixo), analisar_linha(linha_valida()));
    }

    /// Acceptance: entradas inválidas devolvem `None` sem pânico — linha
    /// vazia, truncada, linha de log de erro do próprio proxy e status não
    /// numérico.
    #[test]
    fn linha_malformada_devolve_none() {
        assert!(analisar_linha("").is_none());
        assert!(analisar_linha("   ").is_none());

        // Truncada no meio: falta o campo de bytes após o status.
        let truncada = r#"app.exemplo.interno 10.1.30.44 - - [07/Aug/2026:14:22:31 +0000] "GET /pedidos HTTP/1.1" 200"#;
        assert!(analisar_linha(truncada).is_none());

        // Truncada dentro do bloco entre aspas (sem aspas de fechamento).
        let truncada = r#"app.exemplo.interno 10.1.30.44 - - [07/Aug/2026:14:22:31 +0000] "GET /pedidos HTTP/1"#;
        assert!(analisar_linha(truncada).is_none());

        // Linha de log de erro do próprio proxy, não uma requisição.
        let erro_proxy = "2026/08/07 14:22:31 [error] 1234#1234: connect() failed (111: Connection refused) while connecting to upstream";
        assert!(analisar_linha(erro_proxy).is_none());

        // Status não numérico.
        let status_invalido = r#"app.exemplo.interno 10.1.30.44 - - [07/Aug/2026:14:22:31 +0000] "GET /x HTTP/1.1" duzentos 5126 "-" "-" "-""#;
        assert!(analisar_linha(status_invalido).is_none());

        // Mês desconhecido descarta a linha.
        let mes_invalido = r#"app.exemplo.interno 10.1.30.44 - - [07/XYZ/2026:14:22:31 +0000] "GET /x HTTP/1.1" 200 5126 "-" "-" "-""#;
        assert!(analisar_linha(mes_invalido).is_none());
    }

    /// Acceptance: aspas dentro do user-agent não corrompem o parse — o
    /// status é ancorado posicionalmente (primeiro token após o bloco da
    /// requisição), nunca pela última aspa da linha.
    #[test]
    fn aspas_no_user_agent_nao_corrompem_status_nem_rota() {
        let linha = r#"api.exemplo.interno 10.1.30.44 - - [07/Aug/2026:14:22:31 +0000] "POST /api/v1/dados HTTP/1.1" 201 128 "-" "Mozilla/5.0 "evil"; DROP TABLE" "172.20.0.7:8000""#;
        let requisicao = analisar_linha(linha).expect("aspas no user-agent");
        assert_eq!(requisicao.status, 201);
        assert_eq!(requisicao.rota, "/api/v1/dados");
    }

    /// Acceptance: query strings diferentes caem na mesma rota — o volume
    /// é por rota, não por recurso individual.
    #[test]
    fn query_string_nao_faz_parte_da_rota() {
        let linhas = [
            r#"app.exemplo.interno 10.1.30.44 - - [07/Aug/2026:14:22:31 +0000] "GET /pedidos?id=9 HTTP/1.1" 200 5126 "-" "-" "-""#,
            r#"app.exemplo.interno 10.1.30.44 - - [07/Aug/2026:14:22:32 +0000] "GET /pedidos?id=10 HTTP/1.1" 200 5126 "-" "-" "-""#,
        ];
        let vhosts = agregar(linhas.into_iter());
        let vhost = &vhosts[0];
        assert_eq!(vhost.requisicoes, 2);
        assert_eq!(
            vhost.rotas_top,
            vec![RotaContagem {
                rota: "/pedidos".to_string(),
                requisicoes: 2,
            }]
        );
    }

    /// Acceptance: a série diária sai ordenada por data crescente, com os
    /// totais e erros de cada dia corretos — mesmo com as linhas chegando
    /// fora de ordem cronológica.
    #[test]
    fn serie_diaria_ordenada_com_totais_corretos() {
        let linhas = [
            r#"app.exemplo.interno 10.1.30.44 - - [07/Aug/2026:10:00:00 +0000] "GET /home HTTP/1.1" 500 5126 "-" "-" "-""#,
            r#"app.exemplo.interno 10.1.30.44 - - [05/Aug/2026:09:00:00 +0000] "GET /home HTTP/1.1" 200 5126 "-" "-" "-""#,
            r#"app.exemplo.interno 10.1.30.44 - - [06/Aug/2026:11:00:00 +0000] "GET /contato HTTP/1.1" 404 5126 "-" "-" "-""#,
            r#"app.exemplo.interno 10.1.30.44 - - [05/Aug/2026:10:00:00 +0000] "GET /sobre HTTP/1.1" 403 5126 "-" "-" "-""#,
        ];
        let vhosts = agregar(linhas.into_iter());
        let vhost = &vhosts[0];

        assert_eq!(vhost.requisicoes, 4);
        assert_eq!(vhost.erros_4xx, 2);
        assert_eq!(vhost.erros_5xx, 1);

        let datas: Vec<&str> = vhost.dias.iter().map(|dia| dia.data.as_str()).collect();
        assert_eq!(datas, ["2026-08-05", "2026-08-06", "2026-08-07"]);

        assert_eq!(vhost.dias[0].requisicoes, 2);
        assert_eq!(vhost.dias[0].erros_4xx, 1);
        assert_eq!(vhost.dias[1].requisicoes, 1);
        assert_eq!(vhost.dias[1].erros_4xx, 1);
        assert_eq!(vhost.dias[2].requisicoes, 1);
        assert_eq!(vhost.dias[2].erros_5xx, 1);
    }

    /// Acceptance: `maquinas` sai sem repetição e em ordem, independente da
    /// ordem em que os IPs aparecem nas linhas.
    #[test]
    fn maquinas_sem_repeticao_e_ordenadas() {
        let linhas = [
            r#"app.exemplo.interno 172.20.0.7 - - [05/Aug/2026:09:00:00 +0000] "GET /a HTTP/1.1" 200 5126 "-" "-" "-""#,
            r#"app.exemplo.interno 10.2.0.10 - - [05/Aug/2026:09:00:01 +0000] "GET /b HTTP/1.1" 200 5126 "-" "-" "-""#,
            r#"app.exemplo.interno 10.1.30.44 - - [05/Aug/2026:09:00:02 +0000] "GET /c HTTP/1.1" 200 5126 "-" "-" "-""#,
            r#"app.exemplo.internal 10.2.0.10 - - [05/Aug/2026:09:00:03 +0000] "GET /d HTTP/1.1" 200 5126 "-" "-" "-""#,
        ];
        let vhosts = agregar(linhas.into_iter());
        assert_eq!(
            vhosts[0].maquinas,
            ["10.1.30.44", "10.2.0.10", "172.20.0.7"].map(str::to_string)
        );
    }

    /// Acceptance: `rotas_top` fica limitada a 20 com desempate
    /// determinístico — contagem igual desempata por rota (alfabética),
    /// nunca pela ordem de chegada das linhas.
    #[test]
    fn rotas_top_limitada_a_20_com_desempate_deterministico() {
        let mut linhas: Vec<String> = Vec::new();
        for i in 0..25 {
            linhas.push(format!(
                r#"app.exemplo.interno 10.1.30.44 - - [07/Aug/2026:14:22:31 +0000] "GET /r{i:02} HTTP/1.1" 200 5126 "-" "-" "-""#
            ));
        }
        // A rota /r24 é a mais requisitada: ganha a segunda ocorrência.
        linhas.push(format!(
            r#"app.exemplo.interno 10.1.30.44 - - [07/Aug/2026:14:22:32 +0000] "GET /r24 HTTP/1.1" 200 5126 "-" "-" "-""#
        ));

        let vhosts = agregar(linhas.iter().map(String::as_str));
        let rotas = &vhosts[0].rotas_top;

        assert_eq!(rotas.len(), 20);
        assert_eq!(rotas[0].rota, "/r24");
        assert_eq!(rotas[0].requisicoes, 2);
        // Empate em 1 requisição: a ordem é alfabética ("/r00", "/r01", ...).
        assert_eq!(rotas[1].rota, "/r00");
        assert_eq!(rotas[2].rota, "/r01");
        assert_eq!(rotas.last().expect("20 rotas").rota, "/r18");
    }

    /// Acceptance: um fluxo sem nenhuma linha válida devolve `Vec` vazio —
    /// não um erro: a coleta é sempre parcial e segue em frente.
    #[test]
    fn fluxo_somente_invalido_devolve_lista_vazia() {
        let vhosts = agregar(
            [
                "",
                "linha qualquer sem o formato do proxy",
                "\"GET /x HTTP/1.1\" 200 5126",
            ]
            .into_iter(),
        );
        assert!(vhosts.is_empty());
    }

    /// Acceptance: o `Vec<VHost>` sai ordenado por volume decrescente — o
    /// vhost mais acessado primeiro — e a agregação é independente por
    /// vhost (incluindo as faixas de erro).
    #[test]
    fn vhosts_ordenados_por_volume_decrescente() {
        let linhas = [
            r#"pouco.exemplo.interno 10.1.30.44 - - [05/Aug/2026:09:00:00 +0000] "GET /a HTTP/1.1" 200 5126 "-" "-" "-""#,
            r#"muito.exemplo.interno 10.1.30.44 - - [05/Aug/2026:09:00:00 +0000] "GET /a HTTP/1.1" 200 5126 "-" "-" "-""#,
            r#"muito.exemplo.interno 10.1.30.44 - - [05/Aug/2026:09:00:01 +0000] "GET /b HTTP/1.1" 404 5126 "-" "-" "-""#,
            r#"muito.exemplo.interno 10.1.30.44 - - [05/Aug/2026:09:00:02 +0000] "GET /c HTTP/1.1" 500 5126 "-" "-" "-""#,
        ];
        let vhosts = agregar(linhas.into_iter());

        assert_eq!(vhosts.len(), 2);
        assert_eq!(vhosts[0].vhost, "muito.exemplo.interno");
        assert_eq!(vhosts[0].requisicoes, 3);
        assert_eq!(vhosts[0].erros_4xx, 1);
        assert_eq!(vhosts[0].erros_5xx, 1);
        assert_eq!(vhosts[1].vhost, "pouco.exemplo.interno");
        assert_eq!(vhosts[1].requisicoes, 1);
    }
}
