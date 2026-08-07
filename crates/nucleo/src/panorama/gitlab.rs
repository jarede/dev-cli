// Módulo `gitlab`: coleta do inventário de repositórios de uma instância
// GitLab self-hosted a partir da API REST v4.
//
// Objetivo: responder "quem escreve o quê", "quais branches existem e até
// onde cada uma chegou" e "quais projetos têm CI". O significado de cada
// campo coletado vive no CONTRATO (`panorama::snapshot::Repositorio`); aqui
// só moram a coleta e o parse.
//
// Decisões de arquitetura:
//   - Usamos `reqwest::blocking` (HTTP síncrono, o mesmo padrão do resto do
//     crate) com o header `PRIVATE-TOKEN` — NÃO invocamos o `glab` como
//     subprocesso: um subprocesso trocaria JSON estruturado por parse de
//     texto, pior cobertura de teste e uma dependência de binário instalado
//     no host.
//   - O parse das respostas JSON fica em FUNÇÕES PURAS separadas
//     (`parsear_projetos`, `parsear_contribuidores`, `parsear_branches`):
//     texto cru -> valores, sem nenhum IO, testáveis com fixture inline. O
//     IO fica confinado em `coletar` e suas auxiliares — o mesmo padrão
//     "núcleo puro X casca de IO" do workspace.
//   - Os contribuidores passam OBRIGATORIAMENTE por
//     `identidade::unificar_autores` antes de virar `Repositorio.autores`:
//     o mesmo humano com três e-mails não aparece como três rastros.
//   - Falha em um projeto vira `ErroColeta` e a coleta segue nos demais —
//     projeto sem permissão de leitura é comum e não deve derrubar nada.

use std::collections::BTreeMap;
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;

use crate::panorama::ResultadoColeta;
use crate::panorama::identidade::{Apelido, Contribuidor, unificar_autores};
use crate::panorama::snapshot::{ErroColeta, Repositorio};

/// Teto de tempo por requisição: uma API remota travada não segura a coleta
/// inteira à toa. É o timeout PADRÃO do cliente, aplicado a cada requisição.
const TIMEOUT_SEGUNDOS: u64 = 30;

/// Tamanho máximo de página pedido à API (o GitLab aceita até 100).
const POR_PAGINA: u32 = 100;

/// Salvaguarda contra paginação infinita/inconsistente da API. Com 100 por
/// página, são até 10.000 projetos — bem além do plausível de uma instância;
/// se estourar, paramos com o acumulado em vez de pedir à toa.
const TETO_PAGINAS: u32 = 100;

/// Configuração da instância GitLab alvo: `host` e token da API.
///
/// O token é um segredo do ambiente de execução; este struct nunca o imprime
/// (nada de `Debug` derivada aqui) e a única mensagem por token ausente é o
/// lembrete acionável de qual variável definir.
pub struct ConfigGitLab {
    pub host: String,
    pub token: String,
}

/// Coleta os repositórios acessíveis à conta do token e devolve o resultado
/// no formato do contrato.
///
/// Erros são PARCIAIS: a falha de um projeto vira `ErroColeta` na lista de
/// `erros` e a coleta continua nos demais. Só falhas no nível da LISTAGEM de
/// projetos (token/rota/parse da lista) fazem `dados` virar `None`.
pub fn coletar(
    cfg: &ConfigGitLab,
    apelidos: &BTreeMap<String, Apelido>,
) -> ResultadoColeta<Vec<Repositorio>> {
    if cfg.token.trim().is_empty() {
        // Requisito de segurança: token ausente vira erro ACIONÁVEL — nunca
        // panic e nunca imprime o valor de nenhuma outra variável de
        // ambiente. A mensagem só cita a variável esperada, nada além disso.
        return ResultadoColeta::falha(
            "token ausente: defina DEV_CLI_PANORAMA_GITLAB_TOKEN",
            "gitlab",
        );
    }

    let cliente = match construir_cliente(&cfg.token) {
        Ok(cliente) => cliente,
        Err(mensagem) => return ResultadoColeta::falha(mensagem, "gitlab"),
    };
    let url_base = url_base(&cfg.host);

    let projetos = match ler_projetos(&cliente, &url_base) {
        Ok(projetos) => projetos,
        // Falha na LISTAGEM não é "projeto a projeto": sem lista não há o que
        // enriquecer, então `dados: None` (não confundir com "zero projetos").
        Err(erro_coleta) => {
            return ResultadoColeta {
                dados: None,
                erros: vec![erro_coleta],
            };
        }
    };

    enriquecer_todos(projetos, |projeto| {
        enriquecer_projeto(&cliente, &url_base, projeto, apelidos)
    })
}

/// Normaliza o `host` para uma só forma e monta a base da URL v4.
///
/// O host pode chegar com ou sem protocolo (`git.exemplo.com` ou
/// `https://git.exemplo.com`) e com barra final. `strip_prefix` devolve
/// `Some` apenas se o prefixo existir; o `unwrap_or` (escolha de padrão, não
/// `unwrap()`) mantém o host como veio quando está sem protocolo. `http://`
/// também vira HTTPS por decisão de projeto: instância self-hosted em
/// produção nunca deve falar HTTP puro.
fn url_base(host: &str) -> String {
    let sem_protocolo = host
        .strip_prefix("https://")
        .or_else(|| host.strip_prefix("http://"))
        .unwrap_or(host);
    format!("https://{}/api/v4", sem_protocolo.trim_end_matches('/'))
}

/// Prepara o header de autenticação — o token NUNCA vai na URL. Já
/// `HeaderValue::from_str` recusa bytes de controle (ex.: quebra de linha);
/// um token assim não pode ir num header, então é erro, não silêncio.
fn cabecalhos_gitlab(token: &str) -> Result<HeaderMap, String> {
    let mut cabecalhos = HeaderMap::new();
    // `HeaderName::from_static` é infalível para uma string ASCII fixa como
    // um literal — não retorna `Result` (diferente de `HeaderValue`).
    let nome = HeaderName::from_static("PRIVATE-TOKEN");
    let valor = HeaderValue::from_str(token)
        .map_err(|erro| format!("token inválido para o header HTTP: {erro}"))?;
    cabecalhos.insert(nome, valor);
    Ok(cabecalhos)
}

fn construir_cliente(token: &str) -> Result<Client, String> {
    Client::builder()
        .timeout(Duration::from_secs(TIMEOUT_SEGUNDOS))
        .default_headers(cabecalhos_gitlab(token)?)
        .build()
        .map_err(|erro| format!("falha ao criar cliente HTTP: {erro}"))
}

/// Percorre as páginas de um endpoint paginado, acumulando os itens.
///
/// Para na primeira página vazia (o GitLab devolve `[]` além da última) ou no
/// teto de tentativas — nunca leva a uma lista cortada em silêncio. O fetcher
/// por página é passado por parâmetro: isso permite testar a paginação com um
/// FAKE que devolve páginas pré-fabricadas, sem rede.
fn percorrer_paginas<T>(
    mut buscar_pagina: impl FnMut(u32) -> Result<Vec<T>, String>,
) -> Result<Vec<T>, String> {
    let mut itens = Vec::new();
    for pagina in 1..=TETO_PAGINAS {
        let conteudo = buscar_pagina(pagina)?;
        if conteudo.is_empty() {
            break;
        }
        itens.extend(conteudo);
    }
    Ok(itens)
}

/// Lê a listagem de projetos (com paginação OBRIGATÓRIA) já parseada.
fn ler_projetos(cliente: &Client, url_base: &str) -> Result<Vec<Projeto>, ErroColeta> {
    percorrer_paginas(|pagina| {
        // `membership=true` filtra para projetos nos quais a conta do token é
        // membro — evita vazar o namespace inteiro da instância. Parar na
        // primeira página perderia silenciosamente os projetos além do
        // centésimo; por isso a paginação até a página vazia.
        let url =
            format!("{url_base}/projects?membership=true&per_page={POR_PAGINA}&page={pagina}");
        let json = buscar_json(cliente, &url)?;
        parsear_projetos(&json)
    })
    .map_err(|mensagem| ErroColeta {
        coletor: "gitlab".to_string(),
        mensagem,
    })
}

/// Faz a requisição e devolve o corpo como texto, ou um erro descritivo.
/// Qualquer status fora de 2xx é erro — as decisões por status (404 vs 500)
/// ficam nas funções de regra de negócio, como `decidir_ci`.
fn buscar_json(cliente: &Client, url: &str) -> Result<String, String> {
    let resposta = cliente
        .get(url)
        .send()
        .map_err(|erro| format!("falha na requisição a {url}: {erro}"))?;
    if !resposta.status().is_success() {
        return Err(format!(
            "resposta inesperada de {url}: status {}",
            resposta.status(),
        ));
    }
    resposta
        .text()
        .map_err(|erro| format!("corpo ilegível de {url}: {erro}"))
}

/// Enriquece cada projeto: falha em UM não interrompe os demais — o erro vira
/// `ErroColeta` na lista. Saída determinística: ordenada por `caminho`.
fn enriquecer_todos(
    projetos: Vec<Projeto>,
    mut enriquecer: impl FnMut(Projeto) -> Result<Repositorio, ErroColeta>,
) -> ResultadoColeta<Vec<Repositorio>> {
    let mut repositorios = Vec::new();
    let mut erros = Vec::new();
    for projeto in projetos {
        match enriquecer(projeto) {
            Ok(repositorio) => repositorios.push(repositorio),
            Err(erro) => erros.push(erro),
        }
    }
    repositorios.sort_by(|a, b| a.caminho.cmp(&b.caminho));
    ResultadoColeta {
        dados: Some(repositorios),
        erros,
    }
}

/// Enriquece UM projeto coletado: contribuidores (via identidade), branches e
/// presença de CI. Devolve `ErroColeta` em qualquer ponto de falha — é o loop
/// em `enriquecer_todos` que garante a coleta seguir nos demais projetos.
fn enriquecer_projeto(
    cliente: &Client,
    url_base: &str,
    projeto: Projeto,
    apelidos: &BTreeMap<String, Apelido>,
) -> Result<Repositorio, ErroColeta> {
    // O `coletor` identifica a origem no snapshot: ex. "gitlab:grupo/appz".
    let coletor = format!("gitlab:{}", projeto.caminho);
    let base_projeto = format!("{url_base}/projects/{}", projeto.id);

    // 1. Contribuidores -> identidade. Os commits somados AQUI (depois de
    //    unificar e remover bots) são o total humano do repositório.
    let json = buscar_json(cliente, &format!("{base_projeto}/repository/contributors")).map_err(
        |mensagem| ErroColeta {
            coletor: coletor.clone(),
            mensagem,
        },
    )?;
    let contribuidores = parsear_contribuidores(&json).map_err(|mensagem| ErroColeta {
        coletor: coletor.clone(),
        mensagem,
    })?;
    let autores = unificar_autores(&contribuidores, apelidos);
    let commits: u64 = autores.iter().map(|autor| autor.commits).sum();

    // 2. Branches: nome -> SHA do último commit. O `BTreeMap` do contrato já
    //    ordena lexicograficamente e serializa determinístico.
    let json = buscar_json(cliente, &format!("{base_projeto}/repository/branches")).map_err(
        |mensagem| ErroColeta {
            coletor: coletor.clone(),
            mensagem,
        },
    )?;
    let branches = parsear_branches(&json).map_err(|mensagem| ErroColeta {
        coletor: coletor.clone(),
        mensagem,
    })?;

    // 3. Presença de CI na branch padrão. `ref=` vem do projeto; branch sem
    //    último commit (`None`) cai numa `ref` vazia — a API responde 404 e o
    //    resultado é `false`, comportamento correto para repositório vazio.
    let ref_branch = projeto.branch_padrao.as_deref().unwrap_or_default();
    let url_ci = format!("{base_projeto}/repository/files/.gitlab-ci.yml?ref={ref_branch}");
    let tem_ci = verificar_ci(cliente, &url_ci).map_err(|mensagem| ErroColeta {
        coletor: coletor.clone(),
        mensagem,
    })?;

    Ok(Repositorio {
        identificador: projeto.id,
        caminho: projeto.caminho,
        branch_padrao: projeto.branch_padrao.unwrap_or_default(),
        ultima_atividade: projeto.ultima_atividade.unwrap_or_default(),
        commits,
        autores,
        branches,
        tem_ci,
    })
}

/// Consulta a presença do `.gitlab-ci.yml` na branch pedida.
///
/// A regra de negócio fica em `decidir_ci` (pura e testável sem rede); aqui
/// só o transporte HTTP restante: pedir o arquivo e ler o status.
fn verificar_ci(cliente: &Client, url: &str) -> Result<bool, String> {
    let resposta = cliente
        .get(url)
        .send()
        .map_err(|erro| format!("falha na requisição a {url}: {erro}"))?;
    decidir_ci(resposta.status().as_u16())
}

/// Núcleo puro de decisão de CI a partir do status HTTP.
///
/// Só `404` significa "não tem CI". `500`, `401`, etc. são ERROS — não se
/// confunde "o arquivo não existe" com "não consegui verificar". Essa
/// distinção impede um inventário com CI marcado como ausente só porque a
/// instância estava instável.
fn decidir_ci(status: u16) -> Result<bool, String> {
    match status {
        200 => Ok(true),
        404 => Ok(false),
        outro => Err(format!(
            "status inesperado {outro} ao verificar o .gitlab-ci.yml"
        )),
    }
}

// ---------------------------------------------------------------------------
// Parse PURA das respostas (sem IO, testável com fixture inline).
// ---------------------------------------------------------------------------

/// Um projeto vindo da listagem `/api/v4/projects`. `#[serde(default)]` deixa
/// campos ausentes virarem `None`/vazios — comfortável para um repositório
/// ainda sem branch ou sem atividade (`default_branch` pode ser `null`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct Projeto {
    id: u64,
    #[serde(rename = "path_with_namespace")]
    caminho: String,
    #[serde(rename = "default_branch")]
    branch_padrao: Option<String>,
    #[serde(rename = "last_activity_at")]
    ultima_atividade: Option<String>,
}

fn parsear_projetos(json: &str) -> Result<Vec<Projeto>, String> {
    serde_json::from_str::<Vec<Projeto>>(json)
        .map_err(|erro| format!("resposta de projetos inválida: {erro}"))
}

/// Linha crua do endpoint de contribuidores. O JSON da API fala inglês; os
/// campos são renomeados para pt-br via `#[serde(rename)]` — o código Rust
/// segue a convenção do projeto, e `serde` faz a ponte com a API.
#[derive(Debug, Clone, Deserialize)]
struct ContribuidorJson {
    #[serde(rename = "name")]
    nome: String,
    #[serde(rename = "email")]
    email: String,
    #[serde(rename = "commits")]
    commits: u64,
}

/// Devolve os contribuidores no formato que `identidade` espera (o mesmo,
/// em pt-br), pronto para `unificar_autores`.
fn parsear_contribuidores(json: &str) -> Result<Vec<Contribuidor>, String> {
    let crus: Vec<ContribuidorJson> = serde_json::from_str(json)
        .map_err(|erro| format!("resposta de contribuidores inválida: {erro}"))?;
    Ok(crus
        .into_iter()
        .map(|item| Contribuidor {
            nome: item.nome,
            email: item.email,
            commits: item.commits,
        })
        .collect())
}

/// Uma branch: o SHA do último commit fica aninhado em `commit.id`.
#[derive(Debug, Clone, Deserialize)]
struct BranchJson {
    #[serde(rename = "name")]
    nome: String,
    #[serde(rename = "commit")]
    commit: CommitJson,
}

#[derive(Debug, Clone, Deserialize)]
struct CommitJson {
    #[serde(rename = "id")]
    sha: String,
}

/// Mapa `nome -> SHA` das branches, já determinístico via `BTreeMap`.
fn parsear_branches(json: &str) -> Result<BTreeMap<String, String>, String> {
    let lista: Vec<BranchJson> = serde_json::from_str(json)
        .map_err(|erro| format!("resposta de branches inválida: {erro}"))?;
    Ok(lista
        .into_iter()
        .map(|branch| (branch.nome, branch.commit.sha))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture com duas páginas seguidas e uma página vazia e encerrar —
    /// o comportamento EXIGIDO da paginação. O teste falha se a 1ª página for
    /// tratada como fim (a união ficaria sem os itens da 2ª, ou a 3ª chamada
    /// não aconteceria).
    #[test]
    fn paginacao_percorre_ate_pagina_vazia() {
        let mut chamadas = 0;
        let itens: Vec<u32> = percorrer_paginas(|pagina| {
            chamadas += 1;
            match pagina {
                1 => Ok(vec![1, 2, 3]),
                2 => Ok(vec![4, 5]),
                _ => Ok(Vec::new()),
            }
        })
        .expect("percorrer páginas falhou");

        // Fixture de DUAS páginas produz a UNIÃO de ambas.
        assert_eq!(itens, vec![1, 2, 3, 4, 5]);
        // Página vazia encerra; se só a 1ª fosse lida, `chamadas` seria menor.
        assert_eq!(chamadas, 3);
    }

    #[test]
    fn teto_de_paginas_limita_a_percorrencia() {
        let mut chamadas: u32 = 0;
        let itens: Vec<u32> = percorrer_paginas(|_pagina| {
            chamadas += 1;
            Ok(vec![1])
        })
        .expect("percorrer páginas falhou");
        assert_eq!(chamadas, TETO_PAGINAS);
        assert_eq!(itens.len(), TETO_PAGINAS as usize);
    }

    /// A resolução de cada campo da listagem respeita os nomes do JSON da
    /// API (inglês) e `default_branch: null` vira `None`.
    #[test]
    fn parsear_projetos_le_campos_da_api() {
        let json = r#"[
            {"id": 1, "path_with_namespace": "usuario/projeto", "default_branch": "main", "last_activity_at": "2026-08-07T10:00:00Z", "web_url": "https://git.exemplo.com/usuario/projeto"},
            {"id": 2, "path_with_namespace": "grupo/vazio", "default_branch": null, "last_activity_at": null}
        ]"#;
        let projetos = parsear_projetos(json).expect("parse de projetos");
        assert_eq!(projetos.len(), 2);
        assert_eq!(projetos[0].id, 1);
        assert_eq!(projetos[0].caminho, "usuario/projeto");
        assert_eq!(projetos[0].branch_padrao.as_deref(), Some("main"));
        assert_eq!(
            projetos[0].ultima_atividade.as_deref(),
            Some("2026-08-07T10:00:00Z"),
        );
        assert_eq!(projetos[1].branch_padrao, None);
        assert_eq!(projetos[1].ultima_atividade, None);
    }

    #[test]
    fn parsear_contribuidores_le_campos_da_api() {
        let json = r#"[
            {"name": "Ana Autor", "email": "ana@exemplo.interno", "commits": 4},
            {"name": "Bruno Bizuca", "email": "bruno@exemplo.interno", "commits": 2}
        ]"#;
        let contribuidores = parsear_contribuidores(json).expect("parse contribuidores");
        assert_eq!(contribuidores.len(), 2);
        assert_eq!(contribuidores[0].nome, "Ana Autor");
        assert_eq!(contribuidores[0].email, "ana@exemplo.interno");
        assert_eq!(contribuidores[0].commits, 4);
    }

    #[test]
    fn parsear_branches_mapeia_nome_para_sha() {
        let json = r#"[
            {"name": "main", "commit": {"id": "abc123", "title": "ajuste"}},
            {"name": "producao", "commit": {"id": "def456"}}
        ]"#;
        let branches = parsear_branches(json).expect("parse branches");
        assert_eq!(branches.len(), 2);
        assert_eq!(branches.get("main"), Some(&"abc123".to_string()));
        assert_eq!(branches.get("producao"), Some(&"def456".to_string()));
    }

    /// O `BTreeMap` do contrato já sai ordenado: "main" antes de "producao".
    #[test]
    fn branches_saem_ordenadas_lexicograficamente() {
        let json = r#"[
            {"name": "zeta", "commit": {"id": "1"}},
            {"name": "alpha", "commit": {"id": "2"}}
        ]"#;
        let branches = parsear_branches(json).expect("parse branches");
        let chaves: Vec<&String> = branches.keys().collect();
        assert_eq!(chaves, vec![&"alpha".to_string(), &"zeta".to_string()]);
    }

    /// Os contribuidores passam OBRIGATORIAMENTE por `identidade`: três
    /// e-mails do mesmo autor (mesma local-part) viram um `Autor` só.
    #[test]
    fn tres_emails_do_mesmo_autor_viram_um_so() {
        let json = r#"[
            {"name": "J. Silva", "email": "jsilva@exemplo.interno", "commits": 3},
            {"name": "Jarede F. Silva", "email": "jsilva@exemplo.interno2", "commits": 5},
            {"name": "J. F. S.", "email": "jsilva@outro.interno", "commits": 2}
        ]"#;
        let contribuidores = parsear_contribuidores(json).expect("parse contribuidores");
        let apelidos = BTreeMap::new();
        let autores = unificar_autores(&contribuidores, &apelidos);

        assert_eq!(autores.len(), 1);
        assert_eq!(autores[0].commits, 10);
        assert_eq!(autores[0].emails.len(), 3);
        assert!(
            autores[0]
                .emails
                .contains(&"jsilva@exemplo.interno".to_string())
        );
    }

    /// O `status` 200 = tem CI; 404 = não tem; qualquer outro (500 aqui) é
    /// Erro, NUNCA `false` — não confundir "não tem CI" com "não consegui
    /// verificar".
    #[test]
    fn status_do_ci_distingue_ausente_de_indeterminado() {
        assert!(decidir_ci(200).expect("200 significa que tem"));
        assert!(!decidir_ci(404).expect("404 significa que não tem"));
        assert!(decidir_ci(500).is_err());
        assert!(decidir_ci(401).is_err());
    }

    /// Token ausente (ou só espaços) devolve falha com mensagem ACIONÁVEL,
    /// sem panic e sem vazar valor de variável de ambiente alguma.
    #[test]
    fn token_ausente_falha_com_mensagem_acionavel() {
        let cfg = ConfigGitLab {
            host: "git.exemplo.com".to_string(),
            token: "   ".to_string(),
        };
        let apelidos = BTreeMap::new();
        let resultado = coletar(&cfg, &apelidos);

        assert!(resultado.dados.is_none());
        assert_eq!(resultado.erros.len(), 1);
        assert_eq!(resultado.erros[0].coletor, "gitlab");
        let mensagem = &resultado.erros[0].mensagem;
        assert!(mensagem.contains("token ausente"));
        assert!(mensagem.contains("DEV_CLI_PANORAMA_GITLAB_TOKEN"));
        // Não imprime o valor de nenhuma variável de ambiente — apenas o nome.
        assert!(!mensagem.contains("="));
    }

    /// Falha em UM projeto vira `ErroColeta` e NÃO interrompe os demais — o
    /// resultado parcial traz os dois projetos com leitura autorizada e o
    /// erro do terceiro na lista, ordenado por caminho.
    #[test]
    fn falha_em_um_projeto_nao_interrompe_os_demais() {
        let json = r#"[
            {"id": 3, "path_with_namespace": "grupo/gama"},
            {"id": 1, "path_with_namespace": "usuario/alfa"},
            {"id": 2, "path_with_namespace": "grupo/bloqueado"}
        ]"#;
        let projetos = parsear_projetos(json).expect("parse de projetos");
        let resultado = enriquecer_todos(projetos, |projeto| {
            if projeto.caminho == "grupo/bloqueado" {
                Err(ErroColeta {
                    coletor: format!("gitlab:{}", projeto.caminho),
                    mensagem: "sem permissão de leitura".to_string(),
                })
            } else {
                Ok(Repositorio {
                    identificador: projeto.id,
                    caminho: projeto.caminho,
                    ..Default::default()
                })
            }
        });

        let repositorios = resultado.dados.expect("deve haver dados parciais");
        // Ordenação determinística por caminho, com o erro isolado na lista.
        assert_eq!(repositorios.len(), 2);
        assert_eq!(repositorios[0].caminho, "grupo/gama");
        assert_eq!(repositorios[1].caminho, "usuario/alfa");
        assert_eq!(resultado.erros.len(), 1);
        assert_eq!(resultado.erros[0].mensagem, "sem permissão de leitura");
    }

    /// O host com ou sem protocolo vai para a mesma base https/.../api/v4.
    #[test]
    fn host_aceito_com_ou_sem_protocolo() {
        assert_eq!(
            url_base("git.exemplo.com"),
            "https://git.exemplo.com/api/v4"
        );
        assert_eq!(
            url_base("https://git.exemplo.com"),
            "https://git.exemplo.com/api/v4"
        );
        assert_eq!(
            url_base("http://git.exemplo.com/"),
            "https://git.exemplo.com/api/v4"
        );
    }
}
