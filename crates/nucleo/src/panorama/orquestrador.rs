// Orquestrador do panorama: junta os coletores (sistema, docker, proxy e
// GitLab) num `Snapshot` único, tolerando falha parcial em cada um.
//
// Duas metades separáveis (regra do workspace):
//   - `montar_snapshot` — NÚCLEO PURO/TESTÁVEL: recebe os `ResultadoColeta`
//     de cada coletor e produz o `Snapshot` final, agregando os erros;
//   - `coletar` — CASCA DE IO: constrói `Executor`s a partir da config,
//     dispara cada coletor e entrega os resultados ao montar.
//
// Semântica de saída: `dados: None` apenas quando NENHUM coletor produziu
// nada (todos os hosts inacessíveis + GitLab falho). É esse caso que faz o
// CLI devolver código de saída 1 — falha parcial é sucesso com aviso.

use std::collections::BTreeMap;

use crate::config::{Config, HostPanorama};
use crate::executor::Executor;
use crate::panorama::gitlab::{self, ConfigGitLab};
use crate::panorama::identidade::Apelido;
use crate::panorama::snapshot::{
    Container, InfoHost, Repositorio, Snapshot, VERSAO_SNAPSHOT, VHost,
};
use crate::panorama::{ResultadoColeta, docker, proxy};

/// Instante da coleta em ISO 8601 sem fuso, precisão de segundo — o mesmo
/// valor que nomeia o arquivo (granularidade de hora) e calcula a retenção.
fn agora_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Constrói o `Executor` do host a partir da config. Um host com acesso
/// desconhecido (ou `ssh` sem `destino`) é um problema da config: vira
/// uma falha registrada, e a coleta segue nos demais hosts.
fn executor_do_host(host: &HostPanorama) -> Result<Executor, String> {
    match host.acesso.as_str() {
        "local" => Ok(Executor::Local),
        "ssh" => match host.destino.as_deref() {
            Some(destino) if !destino.is_empty() => Ok(Executor::Ssh(destino.to_string())),
            _ => Err(format!(
                "host '{}': acesso \"ssh\" exige o campo 'destino'",
                host.nome
            )),
        },
        outro => Err(format!(
            "host '{}': acesso desconhecido \"{outro}\"",
            host.nome
        )),
    }
}

/// PURA: agrega o resultado de todos os coletores num `Snapshot`.
/// A entrada é a coleção de `ResultadoColeta` + as listas de hosts — leva
/// os dados parciais e TODOS os erros; o `Snapshot.erros` carrega a mesma
/// lista, para o consumidor saber o que faltou.
pub fn montar_snapshot(
    coletado_em: String,
    sistemas: Vec<ResultadoColeta<InfoHost>>,
    resultados_containers: Vec<ResultadoColeta<Vec<Container>>>,
    resultados_vhosts: Vec<ResultadoColeta<Vec<VHost>>>,
    repositorios: ResultadoColeta<Vec<Repositorio>>,
) -> ResultadoColeta<Snapshot> {
    let mut hosts = Vec::new();
    let mut containers = Vec::new();
    let mut vhosts = Vec::new();
    let mut erros = Vec::new();
    let mut repos = Vec::new();

    // A regra de ouro: `dados` entra, `erros` também — a falha parcial nunca
    // é perdida. `if let Some(partial)` + `if let` 'intrínsecos' (let chain).
    for resultado in sistemas {
        if let Some(dado) = resultado.dados {
            hosts.push(dado);
        }
        erros.extend(resultado.erros);
    }
    for resultado in resultados_containers {
        if let Some(dado) = resultado.dados {
            containers.extend(dado);
        }
        erros.extend(resultado.erros);
    }
    for resultado in resultados_vhosts {
        if let Some(dado) = resultado.dados {
            vhosts.extend(dado);
        }
        erros.extend(resultado.erros);
    }
    if let Some(dado) = repositorios.dados {
        repos = dado;
    }
    erros.extend(repositorios.erros);

    let produziu =
        !(hosts.is_empty() && containers.is_empty() && vhosts.is_empty() && repos.is_empty());

    let snapshot = Snapshot {
        versao: VERSAO_SNAPSHOT,
        coletado_em,
        hosts,
        containers,
        vhosts,
        repositorios: repos,
        erros: erros.clone(),
    };

    // `dados: Some` quando houve produção OU quando nem houve o que coletar
    // (config vazia não é falha). `dados: None` só quando houve erro(s) e
    // nada foi aproveitado — é o que distingue "falha" de "dados indisponíveis".
    if produziu || erros.is_empty() {
        ResultadoColeta {
            dados: Some(snapshot),
            erros,
        }
    } else {
        ResultadoColeta { dados: None, erros }
    }
}

/// CASCA DE IO: executa a coleta completa segundo a config e monta o snapshot.
pub fn coletar(cfg: &Config) -> ResultadoColeta<Snapshot> {
    let coletado_em = agora_iso();

    let mut sistemas = Vec::new();
    let mut containers = Vec::new();
    let mut vhosts = Vec::new();

    for host in &cfg.panorama.hosts {
        let executor = match executor_do_host(host) {
            Ok(executor) => executor,
            Err(mensagem) => {
                // Config de host inválida: registra o erro e segue nos demais.
                let rotulo = format!("config:{}", host.nome);
                sistemas.push(ResultadoColeta::falha(mensagem.clone(), rotulo.clone()));
                containers.push(ResultadoColeta::falha(mensagem.clone(), rotulo.clone()));
                vhosts.push(ResultadoColeta::falha(mensagem.clone(), rotulo.clone()));
                continue;
            }
        };

        let sistema = crate::panorama::sistema::coletar(&executor, &host.nome);
        let docker_result = docker::coletar(&executor, &host.nome);
        sistemas.push(sistema);
        containers.push(docker_result);

        // Proxy: só o host que chocou o `container_proxy` da config contribui
        // vhosts. O parse/agregação é puro (`proxy::agregar`); a obtenção do
        // log é a casca daqui.
        vhosts.push(match &host.container_proxy {
            Some(proxy) => match docker::obter_log_proxy(&executor, proxy) {
                Ok(log) => {
                    let agregado = proxy::agregar(log.lines());
                    ResultadoColeta::parcial(agregado, Vec::new())
                }
                Err(erro) => ResultadoColeta::falha(
                    format!("log do proxy '{proxy}' falhou: {erro}"),
                    format!("proxy:{}", host.nome),
                ),
            },
            // Host sem proxy: nada a produzir — nem dado, nem erro.
            None => ResultadoColeta::parcial(Vec::new(), Vec::new()),
        });
    }

    // GitLab: uma única vez para o inventário inteiro. O token vem só do
    // ambiente (`DEV_CLI_PANORAMA_GITLAB_TOKEN`); ausente vira erro do
    // coletor (falha parcial), com mensagem acionável do próprio gitlab.rs.
    let repositorios = if cfg.panorama.gitlab.host.trim().is_empty() {
        ResultadoColeta::parcial(Vec::new(), Vec::new())
    } else {
        let token = std::env::var("DEV_CLI_PANORAMA_GITLAB_TOKEN").unwrap_or_default();
        let config_gitlab = ConfigGitLab {
            host: cfg.panorama.gitlab.host.clone(),
            token,
        };
        // Sem apelidos configuráveis por enquanto — o mapa vazio é o padrão.
        let apelidos: BTreeMap<String, Apelido> = BTreeMap::new();
        gitlab::coletar(&config_gitlab, &apelidos)
    };

    montar_snapshot(coletado_em, sistemas, containers, vhosts, repositorios)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panorama::snapshot::ErroColeta;

    /// Fábricas mínimas: só o campo que identifica, resto no padrão.
    fn host(nome: &str) -> InfoHost {
        InfoHost {
            nome: nome.to_string(),
            ..InfoHost::default()
        }
    }

    fn container(nome: &str) -> Container {
        Container {
            nome: nome.to_string(),
            ..Container::default()
        }
    }

    fn vhost(nome: &str) -> VHost {
        VHost {
            vhost: nome.to_string(),
            ..VHost::default()
        }
    }

    fn repositorio(caminho: &str) -> Repositorio {
        Repositorio {
            caminho: caminho.to_string(),
            ..Repositorio::default()
        }
    }

    fn erro_simples(rotulo: &str) -> ErroColeta {
        ErroColeta {
            coletor: rotulo.to_string(),
            mensagem: "falhou".to_string(),
        }
    }

    #[test]
    fn tudo_ok_monta_snapshot_completo() {
        let resultado = montar_snapshot(
            "2026-08-07T14:00:00".to_string(),
            vec![ResultadoColeta::parcial(host("app-01"), Vec::new())],
            vec![ResultadoColeta::parcial(
                vec![container("web-1")],
                Vec::new(),
            )],
            vec![ResultadoColeta::parcial(
                vec![vhost("app.exemplo.interno")],
                Vec::new(),
            )],
            ResultadoColeta::parcial(vec![repositorio("grupo/app")], Vec::new()),
        );
        assert!(resultado.erros.is_empty());
        let snap = resultado.dados.expect("tudo ok tem dados");
        assert_eq!(snap.versao, VERSAO_SNAPSHOT);
        assert_eq!(snap.hosts.len(), 1);
        assert_eq!(snap.containers.len(), 1);
        assert_eq!(snap.vhosts.len(), 1);
        assert_eq!(snap.repositorios.len(), 1);
        assert!(snap.erros.is_empty());
    }

    #[test]
    fn falha_parcial_mantem_dados_e_registra_erros() {
        let resultado = montar_snapshot(
            "2026-08-07T14:00:00".to_string(),
            vec![
                ResultadoColeta::parcial(host("app-01"), Vec::new()),
                ResultadoColeta {
                    dados: None,
                    erros: vec![erro_simples("sistema:app-02")],
                },
            ],
            vec![ResultadoColeta {
                dados: None,
                erros: vec![erro_simples("docker:app-02")],
            }],
            vec![ResultadoColeta::parcial(
                vec![vhost("app.exemplo.interno")],
                Vec::new(),
            )],
            ResultadoColeta::falha("token ausente", "gitlab"),
        );
        // Host 1 e vhost continuam no snapshot; os erros viajam também.
        let snap = resultado.dados.expect("falha parcial ainda tem dados");
        assert_eq!(snap.hosts.len(), 1);
        assert_eq!(snap.vhosts.len(), 1);
        assert_eq!(resultado.erros.len(), 3);
        assert_eq!(snap.erros.len(), 3);
    }

    #[test]
    fn um_host_falhando_nao_impede_os_demais() {
        let resultado = montar_snapshot(
            "2026-08-07T14:00:00".to_string(),
            vec![ResultadoColeta::parcial(host("app-01"), Vec::new())],
            vec![
                ResultadoColeta::parcial(vec![container("web-1")], Vec::new()),
                ResultadoColeta {
                    dados: None,
                    erros: vec![erro_simples("docker:app-02")],
                },
            ],
            Vec::new(),
            ResultadoColeta::parcial(Vec::new(), Vec::new()),
        );
        // app-02 falhou, mas app-01 produziu host e container normalmente.
        assert!(resultado.dados.is_some());
        let snap = resultado.dados.expect("parcial");
        assert_eq!(snap.hosts.len(), 1);
        assert_eq!(snap.containers.len(), 1);
        assert_eq!(resultado.erros.len(), 1);
    }

    #[test]
    fn todos_colctores_falham_devolve_dados_none() {
        let resultado = montar_snapshot(
            "2026-08-07T14:00:00".to_string(),
            vec![ResultadoColeta::falha("falhou", "sistema:app-02")],
            vec![ResultadoColeta::falha("falhou", "docker:app-02")],
            vec![ResultadoColeta::falha("falhou", "docker:app-01")],
            ResultadoColeta::falha("falhou", "gitlab"),
        );
        assert!(resultado.dados.is_none());
        assert_eq!(resultado.erros.len(), 4);
    }

    #[test]
    fn config_vazia_nao_e_falha() {
        // Nenhum host, nenhum gitlab: nada a produzir, mas NÃO é falha.
        let resultado = montar_snapshot(
            "2026-08-07T14:00:00".to_string(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            ResultadoColeta::parcial(Vec::new(), Vec::new()),
        );
        assert!(resultado.erros.is_empty());
        let snap = resultado.dados.expect("config vazia é sucesso");
        assert_eq!(snap.versao, VERSAO_SNAPSHOT);
    }
}
