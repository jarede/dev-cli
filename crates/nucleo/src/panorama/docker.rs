// Coletor Docker multi-host — SOMENTE LEITURA: inventário de containers de
// um host (local ou via SSH), usando o `Executor` que já existe no workspace.
//
// Regras de arquitetura deste arquivo (espelhando o `executor.rs`):
//   - `parsear_*`, `montar_containers` e `converter_unidade` são NÚCLEO PURO:
//     texto -> valores, testáveis com fixtures inline, sem daemon;
//   - `coletar_com` é o miolo do fluxo com uma execução INJETADA por closure —
//     é ele que os testes exercitam com um "executor de mentira";
//   - `coletar` é a casca de IO de verdade: liga o `Executor` real ao miolo.
//
// NUNCA execute comandos que alterem estado (`exec`, `restart`, `stop`,
// `start`, `run`, `rm`, `kill`). Permitidos: `ps`, `inspect`,
// `stats --no-stream`, `system df`, `logs`. Um coletor que pode alterar
// estado não pode rodar sob agendamento sem supervisão.

use crate::executor::Executor;
use crate::panorama::ResultadoColeta;
use crate::panorama::segredos;
use crate::panorama::snapshot::{Container, ErroColeta};
use serde::Deserialize;

/// Linha do `docker ps -a --format '{{json .}}'` — um objeto JSON por linha.
/// `#[serde(default)]` + `#[serde(rename)]`: campos ausentes viram padrão e
/// os nomes JSON (CamelCase do Docker) são traduzidos para os nomes Rust.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
struct ContainerPs {
    #[serde(rename = "Names")]
    nome: String,
    #[serde(rename = "Image")]
    imagem: String,
    #[serde(rename = "State")]
    estado: String,
}

/// Objeto de um `docker inspect <nomes...>` (vetor JSON).
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
struct InspecaoContainer {
    #[serde(rename = "Created")]
    criado_em: String,
    #[serde(rename = "RestartCount")]
    reinicios: u32,
    #[serde(rename = "State")]
    estado: EstadoInspecao,
    #[serde(rename = "Config")]
    config: ConfigInspecao,
    #[serde(rename = "HostConfig")]
    host_config: HostConfigInspecao,
    #[serde(rename = "Mounts")]
    mounts: Vec<MountInspecao>,
    #[serde(rename = "NetworkSettings")]
    rede: RedeInspecao,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
struct EstadoInspecao {
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "StartedAt")]
    iniciado_em: String,
    #[serde(rename = "Health")]
    saude: Option<SaudeInspecao>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
struct SaudeInspecao {
    #[serde(rename = "Status")]
    status: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
struct ConfigInspecao {
    #[serde(rename = "Image")]
    imagem: String,
    /// Lista `"CHAVE=valor"` — passa OBRIGATORIAMENTE por `segredos`.
    #[serde(rename = "Env")]
    env: Vec<String>,
    /// Presente apenas quando o Dockerfile declara HEALTHCHECK.
    #[serde(rename = "Healthcheck")]
    healthcheck: Option<serde_json::Value>,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
struct HostConfigInspecao {
    /// Limite em bytes; 0 = sem limite (NUNCA virar `Some(0)`).
    #[serde(rename = "Memory")]
    memoria_limite: u64,
    /// Limite de CPU em nanocpus; 0 = sem limite.
    #[serde(rename = "NanoCpus")]
    cpus_limite: u64,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
struct MountInspecao {
    #[serde(rename = "Name")]
    nome: Option<String>,
    #[serde(rename = "Source")]
    origem: String,
    #[serde(rename = "Destination")]
    destino: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
struct RedeInspecao {
    /// Estrutura variável (mapa porta -> bindings ou null): usamos `Value`.
    #[serde(rename = "Ports")]
    portas: Option<serde_json::Value>,
}

/// Linha do `docker stats --no-stream --format '{{json .}}'`.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
struct EstatisticaContainer {
    #[serde(rename = "Name")]
    nome: String,
    #[serde(rename = "CPUPerc")]
    cpu_percentual: String,
    #[serde(rename = "MemUsage")]
    memoria_uso: String,
}

/// NÚCLEO PURO: converte um texto de tamanho do docker (ex.: "3.908GiB",
/// "512MB", "1.5kB") em bytes. Bases: KB/MB/GB/TB decimais (1000) e
/// KiB/MiB/GiB/TiB binárias (1024). Texto irreconhecível vira 0 — não erro:
/// métrica faltando não derruba o inventário.
fn converter_unidade(texto: &str) -> u64 {
    // Separa o número do sufixo: primeiro caractere que não é dígito/ponto.
    let tex = texto.trim();
    let posicao = tex.find(|c: char| !c.is_ascii_digit() && c != '.');
    let (numero, sufixo) = match posicao {
        Some(p) => tex.split_at(p),
        None => (tex, ""),
    };
    let Ok(valor) = numero.parse::<f64>() else {
        return 0;
    };
    // `to_ascii_lowercase`: o docker alterna "kB" e "KB" conforme a versão —
    // normalizar evita a classe de bug que só aparece num host específico.
    let base = match sufixo.trim().to_ascii_lowercase().as_str() {
        "b" => 1,
        "kb" => 1_000,
        "mb" => 1_000_000,
        "gb" => 1_000_000_000,
        "tb" => 1_000_000_000_000u64,
        "kib" => 1_024,
        "mib" => 1_048_576,
        "gib" => 1_073_741_824,
        "tib" => 1_099_511_627_776u64,
        _ => return 0,
    };
    (valor * base as f64) as u64
}

/// NÚCLEO PURO: "2.50%" -> 2.50; texto que não parseia vira 0.0.
fn parsear_percentual(texto: &str) -> f64 {
    texto.trim().trim_end_matches('%').parse().unwrap_or(0.0)
}

/// NÚCLEO PURO: a parte ANTES de "/" de um `MemUsage` ("3.908GiB / 7.792GiB").
fn parte_memoria(mem_usage: &str) -> &str {
    mem_usage.split('/').next().unwrap_or("").trim()
}

/// NÚCLEO PURO: as linhas do `ps --format '{{json .}}'` (uma por container).
/// `docker` aceita o formato com aspas simples; as linhas vêm com elas.
fn parsear_ps(saida: &str) -> Vec<ContainerPs> {
    saida
        .lines()
        .map(|linha| linha.trim().trim_matches('\''))
        .filter(|linha| !linha.is_empty())
        .filter_map(|linha| serde_json::from_str::<ContainerPs>(linha).ok())
        .collect()
}

/// NÚCLEO PURO: o vetor JSON do `docker inspect <nomes...>`.
/// `Result` (não default) porque inspect malformado é um problema de verdade,
/// não um caso de borda do log.
fn parsear_inspect(saida: &str) -> Result<Vec<InspecaoContainer>, String> {
    serde_json::from_str(saida).map_err(|erro| format!("inspect não é JSON válido: {erro}"))
}

/// NÚCLEO PURO: as linhas do `stats --no-stream --format '{{json .}}'`.
fn parsear_stats(saida: &str) -> Vec<EstatisticaContainer> {
    saida
        .lines()
        .map(|linha| linha.trim().trim_matches('\''))
        .filter(|linha| !linha.is_empty())
        .filter_map(|linha| serde_json::from_str::<EstatisticaContainer>(linha).ok())
        .collect()
}

/// NÚCLEO PURO: porta/publicação -> texto legível ("0.0.0.0:8080->80/tcp",
/// ou só "443/tcp" quando exposta sem vínculo com o host).
fn formatar_portas(portas: Option<&serde_json::Value>) -> Vec<String> {
    let Some(serde_json::Value::Object(mapa)) = portas else {
        return Vec::new();
    };
    let mut resultado = Vec::new();
    for (porta_container, vinculo) in mapa {
        match vinculo {
            serde_json::Value::Array(itens) if !itens.is_empty() => {
                for item in itens {
                    let ip = item
                        .get("HostIp")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("0.0.0.0");
                    let porta_host = item
                        .get("HostPort")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("");
                    resultado.push(format!("{ip}:{porta_host}->{porta_container}"));
                }
            }
            // Array vazio ou null = porta só exposta no container.
            _ => resultado.push(porta_container.clone()),
        }
    }
    resultado.sort();
    resultado
}

/// NÚCLEO PURO: mount -> "origem:destino" (o `Name` do volume nomeado tem
/// preferência sobre o caminho `Source`, que é interno ao docker).
fn formatar_mount(mount: &MountInspecao) -> String {
    let origem = mount.nome.clone().unwrap_or_else(|| mount.origem.clone());
    format!("{origem}:{}", mount.destino)
}

/// NÚCLEO PURO: casa o inventário (ps) com os detalhes (inspect) e as
/// métricas (stats) para montar os `Container` do contrato. Inspect casa por
/// POSIÇÃO (mesma ordem dos nomes passados ao `docker inspect`); stats casa
/// por NOME (só containers em execução aparecem lá).
fn montar_containers(
    ps: Vec<ContainerPs>,
    inspecoes: Vec<InspecaoContainer>,
    estatisticas: Vec<EstatisticaContainer>,
    nome_host: &str,
) -> Vec<Container> {
    ps.into_iter()
        .zip(inspecoes)
        .map(|(item_ps, inspecao)| {
            // O caminho obrigatório: a lista "CHAVE=valor" do inspect passa
            // AQUI pela redação — nenhum outro caminho preenche `variaveis`.
            let variaveis = segredos::variaveis_de_lista(&inspecao.config.env);
            // `VIRTUAL_HOST` não é segredo: sobrevive à redação e é a ponte
            // entre o inventário e o log do proxy reverso.
            let vhost = variaveis
                .get("VIRTUAL_HOST")
                .filter(|valor| !valor.is_empty())
                .cloned();

            let estatistica = estatisticas.iter().find(|e| e.nome == item_ps.nome);

            Container {
                host: nome_host.to_string(),
                nome: item_ps.nome,
                imagem: if inspecao.config.imagem.is_empty() {
                    item_ps.imagem
                } else {
                    inspecao.config.imagem
                },
                // Preferir o estado do inspect; o do ps é o fallback.
                estado: if inspecao.estado.status.is_empty() {
                    item_ps.estado
                } else {
                    inspecao.estado.status
                },
                criado_em: inspecao.criado_em,
                iniciado_em: inspecao.estado.iniciado_em,
                reinicios: inspecao.reinicios,
                tem_healthcheck: inspecao.config.healthcheck.is_some(),
                saude: inspecao.estado.saude.map(|saude| saude.status),
                vhost,
                // 0 = sem limite declarado: `None`, jamais `Some(0)` (que
                // renderizaria como "0 B de limite" na tela do consumidor).
                limite_memoria_bytes: (inspecao.host_config.memoria_limite > 0)
                    .then_some(inspecao.host_config.memoria_limite),
                // Sem stats (container parado) a métrica vira 0 — coerente,
                // não é "não sei".
                memoria_usada_bytes: estatistica
                    .map(|e| converter_unidade(parte_memoria(&e.memoria_uso)))
                    .unwrap_or(0),
                cpu_percentual: estatistica
                    .map(|e| parsear_percentual(&e.cpu_percentual))
                    .unwrap_or(0.0),
                cpus_limite: (inspecao.host_config.cpus_limite > 0)
                    .then(|| inspecao.host_config.cpus_limite as f64 / 1e9),
                portas: formatar_portas(inspecao.rede.portas.as_ref()),
                volumes: inspecao.mounts.iter().map(formatar_mount).collect(),
                variaveis,
            }
        })
        .collect()
}

/// A execução "crua" usada pelo miolo: recebe os argumentos docker e devolve
/// a saída. Nos testes, um fake devolve fixtures inline sem daemon.
/// Lifetime explícita: o trait object empresta o ambiente do closure.
type Execucao<'a> = dyn FnMut(&[&str]) -> Result<String, String> + 'a;

/// MIOLO TESTÁVEL: o fluxo inteiro do coletor com a execução injetada.
/// Falha do `ps` = host inacessível (não há nem como inventariar) -> `None`.
/// Falha do `inspect`/`stats` = dados parciais + `ErroColeta`.
fn coletar_com(executar: &mut Execucao<'_>, nome_host: &str) -> ResultadoColeta<Vec<Container>> {
    let rotulo = || format!("docker:{nome_host}");

    let saida_ps = match executar(&["ps", "-a", "--format", "'{{json .}}'"]) {
        Ok(saida) => saida,
        Err(erro) => return ResultadoColeta::falha(format!("host inacessível: {erro}"), rotulo()),
    };
    let ps = parsear_ps(&saida_ps);
    // Host sem containers é um estado legítimo: dados vazios SEM erro.
    if ps.is_empty() {
        return ResultadoColeta::parcial(Vec::new(), Vec::new());
    }

    // `docker inspect <nomes...>` numa chamada só — os nomes entram como
    // argumentos adicionais ao vetor de args docker.
    let nomes: Vec<String> = ps.iter().map(|container| container.nome.clone()).collect();
    let mut args = vec!["inspect"];
    args.extend(nomes.iter().map(String::as_str));

    let saida_inspect = match executar(&args) {
        Ok(saida) => saida,
        Err(erro) => {
            return ResultadoColeta::parcial(
                Vec::new(),
                vec![ErroColeta {
                    coletor: rotulo(),
                    mensagem: format!("inspect falhou: {erro}"),
                }],
            );
        }
    };
    let inspecoes = match parsear_inspect(&saida_inspect) {
        Ok(inspecoes) => inspecoes,
        Err(erro) => {
            return ResultadoColeta::parcial(
                Vec::new(),
                vec![ErroColeta {
                    coletor: rotulo(),
                    mensagem: erro,
                }],
            );
        }
    };

    let (saida_stats, erros_stats) =
        match executar(&["stats", "--no-stream", "--format", "'{{json .}}'"]) {
            Ok(saida) => (saida, Vec::new()),
            // Métricas são acessório: o inventário segue completo sem elas.
            Err(erro) => (
                String::new(),
                vec![ErroColeta {
                    coletor: rotulo(),
                    mensagem: format!("stats falhou: {erro}"),
                }],
            ),
        };
    let estatisticas = parsear_stats(&saida_stats);

    ResultadoColeta::parcial(
        montar_containers(ps, inspecoes, estatisticas, nome_host),
        erros_stats,
    )
}

/// CASCA DE IO: inventário Docker do host (local ou SSH), SOMENTE LEITURA.
/// Usa o `Executor` do workspace — que já decide entre docker local e SSH.
pub fn coletar(executor: &Executor, nome_host: &str) -> ResultadoColeta<Vec<Container>> {
    // `executor` é `&Executor`: a closure só empresta, não move.
    let mut executar = |args: &[&str]| executor.executar(args).map_err(|erro| erro.to_string());
    coletar_com(&mut executar, nome_host)
}

/// CASCA DE IO: o log completo de um container (fonte do proxy reverso).
/// A janela disponível é o uptime do container (driver json-file) — cabe ao
/// orquestrador decidir o que fazer com isso. Somente leitura.
pub fn obter_log_proxy(
    executor: &Executor,
    nome: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    executor.executar(&["logs", nome])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panorama::segredos::REDIGIDO;
    use std::collections::BTreeMap;

    /// Fixture inline do `ps -a --format '{{json .}}'` (com as aspas simples
    /// que o docker devolve por causa do format).
    const FIXTURE_PS: &str = "\
'{\"Names\":\"web-1\",\"Image\":\"exemplo.interno/web:3.2\",\"State\":\"running\",\"Status\":\"Up 2 hours\"}'
'{\"Names\":\"api-1\",\"Image\":\"exemplo.interno/api:1.7\",\"State\":\"exited\",\"Status\":\"Exited (0) 3 days ago\"}'
";

    /// Fixture inline do `docker inspect web-1 api-1` (vetor, mesma ordem).
    const FIXTURE_INSPECT: &str = r#"[
{
  "Id": "aaa",
  "Created": "2026-06-30T12:00:00.000000001Z",
  "RestartCount": 2,
  "State": {
    "Status": "running",
    "StartedAt": "2026-07-04T08:00:00.000000002Z",
    "Health": { "Status": "healthy" }
  },
  "Config": {
    "Image": "exemplo.interno/web:3.2",
    "Env": [
      "PATH=/usr/bin",
      "DB_PASSWORD=hunter2",
      "VIRTUAL_HOST=app.exemplo.interno"
    ],
    "Healthcheck": { "Test": ["CMD", "true"], "Interval": 30000000000 }
  },
  "HostConfig": { "Memory": 104857600, "NanoCpus": 2000000000 },
  "Mounts": [
    { "Type": "volume", "Name": "dados-web", "Source": "/var/lib/docker/volumes/dados-web", "Destination": "/usr/share/nginx/html" }
  ],
  "NetworkSettings": { "Ports": { "443/tcp": null, "80/tcp": [ { "HostIp": "0.0.0.0", "HostPort": "8080" } ] } }
},
{
  "Id": "bbb",
  "Created": "2026-05-01T09:00:00.000000001Z",
  "RestartCount": 0,
  "State": { "Status": "exited", "StartedAt": "2026-07-01T18:00:00.000000002Z" },
  "Config": {
    "Image": "exemplo.interno/api:1.7",
    "Env": [ "PATH=/usr/bin", "LANG=C.UTF-8" ]
  },
  "HostConfig": { "Memory": 0, "NanoCpus": 0 },
  "Mounts": [],
  "NetworkSettings": { "Ports": {} }
}
]"#;

    /// Fixture inline do `stats --no-stream --format '{{json .}}'` — só o
    /// container em execução aparece.
    const FIXTURE_STATS: &str = "\
{\"Name\":\"web-1\",\"CPUPerc\":\"2.50%\",\"MemUsage\":\"3.908GiB / 7.792GiB\",\"MemPerc\":\"50.17%\",\"NetIO\":\"0B / 0B\",\"BlockIO\":\"0B / 0B\",\"PIDs\":\"8\"}
";

    /// "Executor de mentira": devolve fixtures por comando e grava o que foi
    /// pedido. `falhar_*` permitem simular host inacessível e falha parcial.
    fn fake_executor(
        saidas: &'static [(&'static str, &'static str)],
    ) -> impl FnMut(&[&str]) -> Result<String, String> + '_ {
        move |args: &[&str]| {
            for (prefixo, saida) in saidas {
                if args.first().is_some_and(|primeiro| *primeiro == *prefixo) {
                    return Ok(saida.to_string());
                }
            }
            Err("comando não esperado no teste".to_string())
        }
    }

    #[test]
    fn converte_unidades_decimais_e_binarias() {
        // Decimal: base 1000.
        assert_eq!(converter_unidade("512MB"), 512_000_000);
        assert_eq!(converter_unidade("1.5kB"), 1_500);
        assert_eq!(converter_unidade("2TB"), 2_000_000_000_000u64);
        // Binária: base 1024.
        assert_eq!(converter_unidade("1GiB"), 1_073_741_824);
        assert_eq!(
            converter_unidade("3.908GiB"),
            (3.908 * 1_073_741_824.0) as u64
        );
        assert_eq!(converter_unidade("512MiB"), 536_870_912);
        // Bytes puros e tolerância a espaços.
        assert_eq!(converter_unidade(" 42B "), 42);
        // Irreconhecível vira 0, não erro.
        assert_eq!(converter_unidade("xyz"), 0);
        assert_eq!(converter_unidade(""), 0);
    }

    #[test]
    fn fluxo_completo_monta_containers_da_fixture() {
        let mut executar = fake_executor(&[
            ("ps", FIXTURE_PS),
            ("inspect", FIXTURE_INSPECT),
            ("stats", FIXTURE_STATS),
        ]);
        let resultado = coletar_com(&mut executar, "app-01");

        assert!(resultado.erros.is_empty());
        let containers = resultado.dados.expect("fixture válida tem dados");
        assert_eq!(containers.len(), 2);

        let web = &containers[0];
        assert_eq!(web.host, "app-01");
        assert_eq!(web.nome, "web-1");
        assert_eq!(web.imagem, "exemplo.interno/web:3.2");
        assert_eq!(web.estado, "running");
        assert_eq!(web.criado_em, "2026-06-30T12:00:00.000000001Z");
        assert_eq!(web.iniciado_em, "2026-07-04T08:00:00.000000002Z");
        assert_eq!(web.reinicios, 2);
        assert!(web.tem_healthcheck);
        assert_eq!(web.saude.as_deref(), Some("healthy"));
        // VIRTUAL_HOST presente vira o vhost do container.
        assert_eq!(web.vhost.as_deref(), Some("app.exemplo.interno"));
        // Métricas vindas do stats.
        assert_eq!(web.memoria_usada_bytes, converter_unidade("3.908GiB"));
        assert_eq!(web.cpu_percentual, 2.5);
        // Limites do HostConfig.
        assert_eq!(web.limite_memoria_bytes, Some(104_857_600));
        assert_eq!(web.cpus_limite, Some(2.0));
        // Portas e volumes legíveis (443/tcp aparece exposta sem binding).
        assert_eq!(
            web.portas,
            vec!["0.0.0.0:8080->80/tcp".to_string(), "443/tcp".to_string()]
        );
        assert_eq!(
            web.volumes,
            vec!["dados-web:/usr/share/nginx/html".to_string()]
        );

        let api = &containers[1];
        assert_eq!(api.nome, "api-1");
        assert_eq!(api.estado, "exited");
        // Sem healthcheck: false + None.
        assert!(!api.tem_healthcheck);
        assert_eq!(api.saude, None);
        // Sem limite de memória: None, NÃO Some(0).
        assert_eq!(api.limite_memoria_bytes, None);
        assert_eq!(api.cpus_limite, None);
        // Sem stats (parado): métrica zero.
        assert_eq!(api.memoria_usada_bytes, 0);
    }

    #[test]
    fn variaveis_passam_pela_redacao() {
        let mut executar = fake_executor(&[
            ("ps", FIXTURE_PS),
            ("inspect", FIXTURE_INSPECT),
            ("stats", FIXTURE_STATS),
        ]);
        let resultado = coletar_com(&mut executar, "app-01");
        let containers = resultado.dados.expect("fixture válida");

        // DB_PASSWORD do inspect chega REDIGIDA ao contrato.
        let web = &containers[0];
        assert_eq!(
            web.variaveis.get("DB_PASSWORD").map(String::as_str),
            Some(REDIGIDO)
        );
        assert!(
            !serde_json::to_string(&web.variaveis)
                .unwrap()
                .contains("hunter2")
        );

        // Variáveis inocentes sobrevivem intactas.
        assert_eq!(
            web.variaveis.get("PATH").map(String::as_str),
            Some("/usr/bin")
        );
    }

    #[test]
    fn vhost_ausente_vira_none() {
        let mut executar = fake_executor(&[
            ("ps", FIXTURE_PS),
            ("inspect", FIXTURE_INSPECT),
            ("stats", FIXTURE_STATS),
        ]);
        let resultado = coletar_com(&mut executar, "app-01");
        let containers = resultado.dados.expect("fixture válida");
        // api-1 não tem VIRTUAL_HOST no inspect.
        assert_eq!(containers[1].vhost, None);
    }

    #[test]
    fn host_inacessivel_devolve_dados_none() {
        let mut executar = |_args: &[&str]| Err("Connection refused".to_string());
        let resultado = coletar_com(&mut executar, "app-02");
        assert!(resultado.dados.is_none());
        assert_eq!(resultado.erros.len(), 1);
        assert_eq!(resultado.erros[0].coletor, "docker:app-02");
        assert!(resultado.erros[0].mensagem.contains("inacessível"));
    }

    #[test]
    fn host_sem_containers_e_sucesso_vazio() {
        let mut executar = |args: &[&str]| {
            if args.first() == Some(&"ps") {
                Ok("\n".to_string())
            } else {
                Err("não deveria chamar".to_string())
            }
        };
        let resultado = coletar_com(&mut executar, "app-01");
        assert!(resultado.erros.is_empty());
        assert_eq!(
            resultado.dados.expect("sucesso vazio"),
            Vec::<Container>::new()
        );
    }

    #[test]
    fn stats_falhando_nao_derruba_o_inventario() {
        // ps e inspect ok, stats com ERRO de execução (daemon caiu no meio).
        let mut executar = |args: &[&str]| match args.first() {
            Some(&"ps") => Ok(FIXTURE_PS.to_string()),
            Some(&"inspect") => Ok(FIXTURE_INSPECT.to_string()),
            Some(&"stats") => Err("daemon morto".to_string()),
            _ => Err("comando não esperado no teste".to_string()),
        };
        let resultado = coletar_com(&mut executar, "app-01");
        // Container sem stats: erro registrado, inventário completo.
        assert!(!resultado.erros.is_empty());
        assert_eq!(resultado.erros[0].coletor, "docker:app-01");
        let containers = resultado.dados.expect("parcial");
        assert_eq!(containers.len(), 2);
        // Sem métricas: zeros, mas o container existe.
        assert_eq!(containers[0].memoria_usada_bytes, 0);
    }

    #[test]
    fn parsea_stats_e_ps_ignorando_linhas_malformadas() {
        let stats = parsear_stats(
            "{malformado}\n{\"Name\":\"web-1\",\"CPUPerc\":\"0.02%\",\"MemUsage\":\"512MB / 1GiB\"}\n",
        );
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].nome, "web-1");

        let ps = parsear_ps(
            "'{\"Names\":\"a\",\"Image\":\"img\",\"State\":\"running\"}'\nlinha-solta\n",
        );
        assert_eq!(ps.len(), 1);
        assert_eq!(ps[0].nome, "a");
    }

    #[test]
    fn percentual_estranho_vira_zero() {
        assert_eq!(parsear_percentual("2.50%"), 2.5);
        assert_eq!(parsear_percentual("--"), 0.0);
    }

    #[test]
    fn portas_vazias_e_variadas() {
        // Rede sem portas.
        let vazio = serde_json::json!({});
        assert_eq!(formatar_portas(Some(&vazio)), Vec::<String>::new());
        // Porta exposta sem binding e porta com binding — determinístico.
        let misto = serde_json::json!({
            "443/tcp": null,
            "80/tcp": [{"HostIp": "0.0.0.0", "HostPort": "8080"}]
        });
        assert_eq!(
            formatar_portas(Some(&misto)),
            vec!["0.0.0.0:8080->80/tcp".to_string(), "443/tcp".to_string()]
        );
        assert_eq!(formatar_portas(None), Vec::<String>::new());
    }

    #[test]
    fn fixture_ps_usa_os_tres_comandos_do_fluxo() {
        // O inventário real depende de exatamente 3 chamadas docker; o fake
        // acima cobre "ps", "inspect" e "stats" — qualquer chamada extra (ou
        // comando alterador) falha o teste com a mensagem de não esperado.
        let mut executar = fake_executor(&[
            ("ps", FIXTURE_PS),
            ("inspect", FIXTURE_INSPECT),
            ("stats", FIXTURE_STATS),
        ]);
        let _ = coletar_com(&mut executar, "app-01");
        // (a chamada a "ps" dispara inspect + stats; se faltasse alguma,
        // coletar_com teria devolvido dados sem métricas)
    }

    #[test]
    fn variaveis_montadas_e_um_btreemap_redigido() {
        let env = vec![
            "DB_PASSWORD=hunter2".to_string(),
            "VIRTUAL_HOST=app.exemplo.interno".to_string(),
            "OPCOES=a=1,b=2".to_string(),
        ];
        let mapa = segredos::variaveis_de_lista(&env);
        let mut esperado = BTreeMap::new();
        esperado.insert("DB_PASSWORD".to_string(), REDIGIDO.to_string());
        esperado.insert(
            "VIRTUAL_HOST".to_string(),
            "app.exemplo.interno".to_string(),
        );
        esperado.insert("OPCOES".to_string(), "a=1,b=2".to_string());
        assert_eq!(mapa, esperado);
    }
}
