// A API HTTP do dev-server. Só APRESENTAÇÃO/serialização: toda a lógica
// (coleta, métricas, severidade) vive no `nucleo` — este arquivo apenas
// traduz HTTP <-> chamadas de função, como o `render.rs` do CLI traduz
// structs <-> texto colorido.
//
// Concorrência: `rusqlite::Connection` não é `Sync`, então a conexão da API
// fica atrás de um `Mutex` compartilhado por `Arc`. Os handlers NÃO têm
// `await` entre pegar o lock e soltar, então nunca "dormem" segurando o
// mutex — o lock dura só a consulta SQL (rápida). O coletor escreve por
// OUTRA conexão; o modo WAL do SQLite deixa leitor e escritor conviverem.
// docs: https://doc.rust-lang.org/book/ch16-03-shared-state.html
// docs: https://docs.rs/axum/latest/axum/#sharing-state-with-handlers

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use nucleo::coletor::agora_unix;
use nucleo::config::Config;
use nucleo::db::{
    Alerta, ErroLog, HistoricoContainer, LinhaLog, alertas_recentes, carregar_linhas_janela,
    erros_desde, historico_por_hora, resumo_janela,
};
use nucleo::metricas::{ResumoContainer, Severidade, severidade};

/// Estado compartilhado entre todos os handlers da API.
// `#[derive(Clone)]`: o axum clona o estado para cada handler; `Arc` faz o
// clone ser barato (só incrementa o contador de referências, não copia a
// conexão nem a config).
// docs: https://doc.rust-lang.org/std/sync/struct.Arc.html
#[derive(Clone)]
pub struct EstadoApi {
    pub db: Arc<Mutex<Connection>>,
    pub config: Arc<Config>,
    /// Diretório onde as suítes de teste ficam persistidas (arquivos
    /// `<id>.toml`). Configurável via `[servidor] testes_dir` no TOML
    /// ou `DEV_CLI_SERVIDOR_TESTES_DIR`; default: `/etc/dev-cli/testes`.
    pub testes_dir: PathBuf,
    /// Execuções de testes em andamento (id_execucao -> Execucao).
    /// VIVE AQUI no EstadoApi para que o main.rs monte UM router só
    /// (sem precisar mergear dois Routers com estados diferentes).
    /// `tokio::sync::Mutex` não é necessário — o lock é segurado só
    /// durante o `HashMap::insert`/`get`, ambos instantâneos, e o
    /// callback do runner não tem `.await` no meio.
    pub execucoes: Arc<Mutex<HashMap<String, nucleo::testes::Execucao>>>,
}

/// Monta o `Router` com todas as rotas da API.
/// Separado do `main` para os testes montarem o mesmo router com um banco
/// em memória e chamarem os handlers SEM subir um servidor TCP.
pub fn criar_rotas(estado: EstadoApi) -> Router {
    Router::new()
        .route("/api/saude", get(saude))
        .route("/api/config", get(config_efetiva))
        .route("/api/containers", get(listar_containers))
        // `{nome}` é a sintaxe de path param do axum 0.8 (era `:nome` até
        // o 0.7) — o valor chega no handler pelo extractor `Path`.
        // docs: https://docs.rs/axum/latest/axum/extract/struct.Path.html
        .route("/api/containers/{nome}/linhas", get(listar_linhas))
        .route("/api/containers/historico", get(listar_historico))
        .route("/api/alertas", get(listar_alertas))
        .route("/api/erros", get(listar_erros))
        .route("/api/ia/custos", get(crate::ia::custos))
        .route("/api/ia/cambio", get(crate::ia::cambio))
        // `.with_state`: injeta o estado; os handlers o recebem via o
        // extractor `State<EstadoApi>`.
        .with_state(estado)
}

/// Resposta do health check.
#[derive(Serialize)]
struct Saude {
    status: &'static str,
}

/// GET /api/saude — health check para o systemd/monitoramento.
/// `Json<T>` implementa `IntoResponse`: serializa `T` e põe o Content-Type.
// docs: https://docs.rs/axum/latest/axum/struct.Json.html
async fn saude() -> Json<Saude> {
    Json(Saude { status: "ok" })
}

/// GET /api/config — a `Config` EFETIVA que o servidor está usando (depois
/// de aplicar flags > env > arquivo > defaults, exatamente a precedência de
/// `Config::carregar`), para a tela Configuração do portal parar de mostrar
/// valores chumbados no React (porta 8787, intervalo 30s...) que podem não
/// bater com o que o operador configurou de verdade. Somente-leitura: o
/// servidor hoje só LÊ o TOML — não existe endpoint de escrita.
async fn config_efetiva(State(estado): State<EstadoApi>) -> Json<Config> {
    // `(*estado.config).clone()`: `estado.config` é um `Arc<Config>`
    // (barato de clonar A REFERÊNCIA); aqui precisamos do VALOR de dentro
    // para o axum poder serializá-lo como corpo da resposta — daí o
    // desreferenciar (`*`) antes de clonar os dados em si.
    // docs: https://doc.rust-lang.org/std/sync/struct.Arc.html
    Json((*estado.config).clone())
}

/// Query string aceita por `/api/containers` (`?janela_min=60`).
// `Option`: parâmetro ausente = usa o default da config, igual ao dashboard.
#[derive(Deserialize)]
struct ParamsJanela {
    janela_min: Option<u64>,
}

/// Um container na resposta da API: o resumo do nucleo + a severidade já
/// calculada (o portal da Fase 3 não deve reimplementar a classificação).
#[derive(Serialize)]
struct ContainerApi {
    // `flatten`: os campos de `ResumoContainer` aparecem no MESMO objeto
    // JSON, sem um sub-objeto "resumo" — a resposta fica plana.
    // docs: https://serde.rs/attr-flatten.html
    #[serde(flatten)]
    resumo: ResumoContainer,
    severidade: Severidade,
}

/// GET /api/containers — o dashboard em JSON: resumo por container na
/// janela, classificado e ordenado dos piores para os melhores.
async fn listar_containers(
    State(estado): State<EstadoApi>,
    Query(params): Query<ParamsJanela>,
) -> Result<Json<Vec<ContainerApi>>, (StatusCode, String)> {
    let janela_min = params.janela_min.unwrap_or(estado.config.coleta.janela_min);
    let corte = agora_unix() - (janela_min as i64) * 60;

    // `lock()` devolve `Err` se outra thread deu panic segurando o mutex
    // (mutex "envenenado") — improvável aqui, mas viramos 500 em vez de
    // `unwrap()` (proibido fora de teste).
    // docs: https://doc.rust-lang.org/std/sync/struct.Mutex.html#poisoning
    let conn = estado.db.lock().map_err(erro_interno)?;
    let resumos = resumo_janela(&conn, corte).map_err(erro_interno)?;

    let mut lista: Vec<ContainerApi> = resumos
        .into_iter()
        .map(|resumo| {
            let sev = severidade(&resumo, &estado.config.limiares);
            ContainerApi {
                resumo,
                severidade: sev,
            }
        })
        .collect();
    // Piores primeiro (Severidade deriva Ord: Verde < ... < Parado), nome
    // como desempate — a MESMA ordenação do dashboard TUI.
    lista.sort_by(|a, b| {
        b.severidade
            .cmp(&a.severidade)
            .then(a.resumo.nome.cmp(&b.resumo.nome))
    });
    Ok(Json(lista))
}

/// Query string de `/api/containers/{nome}/linhas`.
#[derive(Deserialize)]
struct ParamsLinhas {
    nivel: Option<String>,
    limite: Option<usize>,
    janela_min: Option<u64>,
}

/// GET /api/containers/{nome}/linhas — drill-down: as linhas de log cruas
/// do container na janela (equivalente à tela de linhas da TUI).
async fn listar_linhas(
    State(estado): State<EstadoApi>,
    Path(nome): Path<String>,
    Query(params): Query<ParamsLinhas>,
) -> Result<Json<Vec<LinhaLog>>, (StatusCode, String)> {
    let janela_min = params.janela_min.unwrap_or(estado.config.coleta.janela_min);
    let corte = agora_unix() - (janela_min as i64) * 60;
    let limite = params.limite.unwrap_or(100);

    let conn = estado.db.lock().map_err(erro_interno)?;
    // `as_deref()`: Option<String> -> Option<&str>, emprestando sem clonar.
    // docs: https://doc.rust-lang.org/std/option/enum.Option.html#method.as_deref
    let linhas = carregar_linhas_janela(&conn, &nome, params.nivel.as_deref(), corte, limite)
        .map_err(erro_interno)?;
    Ok(Json(linhas))
}

/// Query string de `/api/containers/historico` — janela em HORAS (default
/// 24), por consistência com o endpoint que renderiza o strip da tela
/// "Histórico" do portal (24 células = 1 strip de 24h).
#[derive(Deserialize)]
struct ParamsHistorico {
    horas: Option<i64>,
}

/// GET /api/containers/historico — contagem de erros+críticos agrupada por
/// hora nas últimas `?horas=N` (default 24), um strip por container.
/// `/containers/historico` em vez de `/containers/{nome}/historico` para
/// devolver TODOS de uma vez (o portal quer uma lista) — a API da Fase 2
/// não paginava, mantém o mesmo padrão "lista enxuta".
/// Teto de `?horas`: 30 dias. Sem teto, `?horas=100000000` faria
/// `historico_por_hora` alocar um `Vec::with_capacity` proporcional a esse
/// número POR CONTAINER — um jeito barato de derrubar o processo por OOM a
/// partir de um único parâmetro de query. 30 dias já é bem mais que a
/// tela "Histórico" (24h) precisa; existe folga para uso futuro (ex.: um
/// seletor de período maior), não para qualquer valor arbitrário.
const HORAS_HISTORICO_MAX: i64 = 24 * 30;

async fn listar_historico(
    State(estado): State<EstadoApi>,
    Query(params): Query<ParamsHistorico>,
) -> Result<Json<Vec<HistoricoContainer>>, (StatusCode, String)> {
    // `.clamp(1, HORAS_HISTORICO_MAX)`: evita tanto janela zero/negativa
    // (que a query do nucleo interpretaria como "tudo") quanto um valor
    // gigante que aloca memória sem limite — ver `HORAS_HISTORICO_MAX`.
    let horas = params.horas.unwrap_or(24).clamp(1, HORAS_HISTORICO_MAX);
    let conn = estado.db.lock().map_err(erro_interno)?;
    let hist = historico_por_hora(&conn, horas, agora_unix()).map_err(erro_interno)?;
    Ok(Json(hist))
}

/// Query string de `/api/alertas`.
#[derive(Deserialize)]
struct ParamsAlertas {
    limite: Option<usize>,
}

/// GET /api/alertas — containers que pararam/reiniciaram dentro do período
/// de retenção do banco (o prune apaga o que for mais velho que isso).
async fn listar_alertas(
    State(estado): State<EstadoApi>,
    Query(params): Query<ParamsAlertas>,
) -> Result<Json<Vec<Alerta>>, (StatusCode, String)> {
    let corte = agora_unix() - (estado.config.coleta.retencao_horas as i64) * 3600;
    let limite = params.limite.unwrap_or(100);

    let conn = estado.db.lock().map_err(erro_interno)?;
    let alertas = alertas_recentes(&conn, corte, limite).map_err(erro_interno)?;
    Ok(Json(alertas))
}

/// Query string de `/api/erros` — cursor incremental por `id`.
/// Ausente/`0` = desde o início; `limite` default 100.
#[derive(Deserialize)]
struct ParamsErros {
    desde_id: Option<i64>,
    limite: Option<usize>,
}

/// GET /api/erros — feed global de ERROR/CRIT de qualquer container, com
/// cursor por `id` (só o que é novo desde a última busca do cliente).
/// Sem filtro de janela de tempo: o cursor já resolve "o que é novo".
async fn listar_erros(
    State(estado): State<EstadoApi>,
    Query(params): Query<ParamsErros>,
) -> Result<Json<Vec<ErroLog>>, (StatusCode, String)> {
    let desde_id = params.desde_id.unwrap_or(0);
    let limite = params.limite.unwrap_or(100);

    let conn = estado.db.lock().map_err(erro_interno)?;
    let erros = erros_desde(&conn, desde_id, limite).map_err(erro_interno)?;
    Ok(Json(erros))
}

/// Converte qualquer erro exibível numa resposta 500 com a mensagem no
/// corpo. Usado com `.map_err(erro_interno)?` nos handlers.
// Genérico em `E: Display` para aceitar `Box<dyn Error>`, `PoisonError`...
// docs: https://doc.rust-lang.org/std/fmt/trait.Display.html
fn erro_interno<E: std::fmt::Display>(erro: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, erro.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    // `BodyExt::collect`: junta os chunks do corpo da resposta em bytes.
    // docs: https://docs.rs/http-body-util/latest/http_body_util/trait.BodyExt.html
    use http_body_util::BodyExt;
    // `ServiceExt::oneshot`: chama o Router como uma função (uma request,
    // uma response), sem abrir porta TCP — o jeito padrão de testar axum.
    // docs: https://docs.rs/tower/latest/tower/trait.ServiceExt.html#method.oneshot
    use tower::ServiceExt;

    use nucleo::db::init_db;

    /// Estado de teste: banco em memória com o schema criado.
    fn estado_teste() -> EstadoApi {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        EstadoApi {
            db: Arc::new(Mutex::new(conn)),
            config: Arc::new(Config::default()),
            testes_dir: std::env::temp_dir(),
            execucoes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// GET na rota e corpo parseado como JSON (helper dos testes).
    async fn get_json(rotas: Router, uri: &str) -> (StatusCode, serde_json::Value) {
        let resposta = rotas
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resposta.status();
        let corpo = resposta.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&corpo).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    #[tokio::test]
    async fn saude_responde_ok() {
        let (status, json) = get_json(criar_rotas(estado_teste()), "/api/saude").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn config_efetiva_devolve_a_config_do_estado() {
        let mut config = Config::default();
        config.coleta.intervalo_seg = 45;
        config.servidor.bind = "0.0.0.0:9999".to_string();
        let estado = EstadoApi {
            db: Arc::new(Mutex::new({
                let conn = Connection::open_in_memory().unwrap();
                init_db(&conn).unwrap();
                conn
            })),
            config: Arc::new(config),
            testes_dir: std::env::temp_dir(),
            execucoes: Arc::new(Mutex::new(HashMap::new())),
        };

        let (status, json) = get_json(criar_rotas(estado), "/api/config").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["coleta"]["intervalo_seg"], 45);
        assert_eq!(json["servidor"]["bind"], "0.0.0.0:9999");
    }

    #[tokio::test]
    async fn rota_desconhecida_e_404() {
        let (status, _) = get_json(criar_rotas(estado_teste()), "/nao-existe").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// Popula o banco do estado com um container "app" cheio de erros e um
    /// "zen" saudável, ambos coletados agora (dentro de qualquer janela).
    fn semear_containers(estado: &EstadoApi) {
        let conn = estado.db.lock().unwrap();
        let agora = nucleo::coletor::agora_unix();
        conn.execute(
            "INSERT INTO containers (name, status, last_collected_at, uptime, criado_em)
             VALUES ('app', 'running', ?1, 'Up 1 day', ''),
                    ('zen', 'running', ?1, 'Up 2 days', '')",
            rusqlite::params![agora],
        )
        .unwrap();
        // 10 CRITICAL para o "app" ficar Vermelho; "zen" sem nada = Verde.
        conn.execute(
            "INSERT INTO log_counts (container_name, level, count, collected_at)
             VALUES ('app', 'CRITICAL', 10, ?1)",
            rusqlite::params![agora],
        )
        .unwrap();
    }

    #[tokio::test]
    async fn containers_lista_com_severidade_e_piores_primeiro() {
        let estado = estado_teste();
        semear_containers(&estado);

        let (status, json) = get_json(criar_rotas(estado), "/api/containers").await;
        assert_eq!(status, StatusCode::OK);

        let lista = json.as_array().unwrap();
        assert_eq!(lista.len(), 2);
        // "app" (Vermelho) vem antes de "zen" (Verde), como no dashboard.
        assert_eq!(lista[0]["nome"], "app");
        assert_eq!(lista[0]["severidade"], "Vermelho");
        assert_eq!(lista[0]["crits"], 10);
        assert_eq!(lista[1]["nome"], "zen");
        assert_eq!(lista[1]["severidade"], "Verde");
    }

    #[tokio::test]
    async fn linhas_filtra_por_nivel_do_container() {
        let estado = estado_teste();
        {
            let conn = estado.db.lock().unwrap();
            let agora = nucleo::coletor::agora_unix();
            conn.execute(
                "INSERT INTO log_lines (container_name, level, line, collected_at)
                 VALUES ('app', 'ERROR', 'deu ruim', ?1),
                        ('app', 'INFO', 'tudo bem', ?1)",
                rusqlite::params![agora],
            )
            .unwrap();
        }

        let rotas = criar_rotas(estado);
        let (status, json) =
            get_json(rotas.clone(), "/api/containers/app/linhas?nivel=ERROR").await;
        assert_eq!(status, StatusCode::OK);
        let lista = json.as_array().unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0]["linha"], "deu ruim");

        // Sem filtro: as duas linhas.
        let (_, json) = get_json(rotas, "/api/containers/app/linhas").await;
        assert_eq!(json.as_array().unwrap().len(), 2);
    }

    /// Monta o mesmo fallback do `main.rs`: `ServeDir` com `not_found_service`
    /// apontando pro `index.html` — o que faz o SPA funcionar em rotas do
    /// react-router (`/historico`, `/ia`...) que não existem como arquivo.
    fn rotas_com_portal(dir: &std::path::Path) -> Router {
        criar_rotas(estado_teste()).fallback_service(
            tower_http::services::ServeDir::new(dir)
                .not_found_service(tower_http::services::ServeFile::new(dir.join("index.html"))),
        )
    }

    #[tokio::test]
    async fn portal_estatico_e_servido_como_fallback() {
        let dir = std::env::temp_dir().join("dev-cli-teste-portal");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<h1>portal</h1>").unwrap();

        let rotas = rotas_com_portal(&dir);

        let resposta = rotas
            .clone()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resposta.status(), StatusCode::OK);

        let (status, json) = get_json(rotas, "/api/saude").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn rota_do_react_router_cai_no_index_html() {
        // Regressão do achado 1: sem `not_found_service`, recarregar
        // `/historico` em produção devolvia 404 (o `ServeDir` só conhece
        // arquivos reais do build) em vez do `index.html` que deixa o
        // react-router assumir o roteamento no cliente.
        let dir = std::env::temp_dir().join("dev-cli-teste-portal-spa");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), "<h1>portal</h1>").unwrap();

        let rotas = rotas_com_portal(&dir);
        let resposta = rotas
            .oneshot(
                Request::builder()
                    .uri("/historico")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // `not_found_service` (padrão recomendado do próprio tower-http para
        // SPAs) força o status para 404 mesmo servindo o `index.html` no
        // corpo — o que importa aqui é o CORPO: é isso que deixa o
        // react-router assumir e trocar de tela no navegador. Uma
        // navegação direta do browser (recarregar, colar a URL) renderiza
        // o corpo HTML normalmente mesmo com status não-2xx; só `fetch()`
        // trataria 404 como falha, e não é o caso de uma navegação de página.
        assert_eq!(resposta.status(), StatusCode::NOT_FOUND);
        let corpo = resposta.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&corpo[..], b"<h1>portal</h1>");
    }

    #[tokio::test]
    async fn erros_cursor_respeita_desde_id() {
        let estado = estado_teste();
        {
            let conn = estado.db.lock().unwrap();
            let agora = nucleo::coletor::agora_unix();
            conn.execute(
                "INSERT INTO log_lines (container_name, level, line, collected_at)
                 VALUES ('app', 'ERROR', 'deu ruim', ?1),
                        ('app', 'INFO', 'tudo bem', ?1),
                        ('zen', 'CRITICAL', 'caiu', ?1)",
                rusqlite::params![agora],
            )
            .unwrap();
        }

        let rotas = criar_rotas(estado);
        // desde_id ausente: todos os erros (INFO fica de fora).
        let (status, json) = get_json(rotas.clone(), "/api/erros").await;
        assert_eq!(status, StatusCode::OK);
        let lista = json.as_array().unwrap();
        assert_eq!(lista.len(), 2);
        assert_eq!(lista[0]["container"], "app");
        assert_eq!(lista[0]["nivel"], "ERROR");
        assert_eq!(lista[0]["linha"], "deu ruim");
        assert!(lista[0]["id"].as_i64().unwrap() > 0);
        assert!(lista[0]["collected_at"].as_i64().is_some());
        assert_eq!(lista[1]["container"], "zen");
        assert_eq!(lista[1]["nivel"], "CRITICAL");

        // desde_id no último id: nada novo.
        let ultimo_id = lista[1]["id"].as_i64().unwrap();
        let (_, json) = get_json(rotas, &format!("/api/erros?desde_id={ultimo_id}")).await;
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn alertas_recentes_saem_no_json() {
        let estado = estado_teste();
        {
            let conn = estado.db.lock().unwrap();
            let agora = nucleo::coletor::agora_unix();
            conn.execute(
                "INSERT INTO alerts (container_name, alert_type, message, created_at)
                 VALUES ('app', 'stopped', 'Container parou', ?1)",
                rusqlite::params![agora],
            )
            .unwrap();
        }

        let (status, json) = get_json(criar_rotas(estado), "/api/alertas").await;
        assert_eq!(status, StatusCode::OK);
        let lista = json.as_array().unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0]["container"], "app");
        assert_eq!(lista[0]["tipo"], "stopped");
    }

    #[tokio::test]
    async fn historico_endpoint_devolve_24_celulas_por_container() {
        let estado = estado_teste();
        {
            let conn = estado.db.lock().unwrap();
            let agora = nucleo::coletor::agora_unix();
            // Container com 5 ERRORs há 1h.
            conn.execute(
                "INSERT INTO containers (name, status, last_collected_at, uptime, criado_em)
                 VALUES ('app', 'running', ?1, 'Up 1 day', '')",
                rusqlite::params![agora],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO log_counts (container_name, level, count, collected_at)
                 VALUES ('app', 'ERROR', 5, ?1)",
                rusqlite::params![agora - 3600],
            )
            .unwrap();
        }

        let (status, json) = get_json(criar_rotas(estado), "/api/containers/historico").await;
        assert_eq!(status, StatusCode::OK);
        let lista = json.as_array().unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0]["nome"], "app");
        let horas = lista[0]["horas"].as_array().unwrap();
        assert_eq!(horas.len(), 24);
        // Soma = 5 (o resto é 0). O total é a soma simples.
        assert_eq!(lista[0]["total"], 5);
        // A hora com 5 contagens fica na célula de índice 1 (a anterior).
        assert!(horas.iter().any(|c| c["quantidade"] == 5));
    }

    #[tokio::test]
    async fn historico_endpoint_respeita_horas_custom() {
        let estado = estado_teste();
        {
            let conn = estado.db.lock().unwrap();
            let agora = nucleo::coletor::agora_unix();
            conn.execute(
                "INSERT INTO containers (name, status, last_collected_at, uptime, criado_em)
                 VALUES ('app', 'running', ?1, 'Up 1 day', '')",
                rusqlite::params![agora],
            )
            .unwrap();
        }

        let (status, json) =
            get_json(criar_rotas(estado), "/api/containers/historico?horas=6").await;
        assert_eq!(status, StatusCode::OK);
        let horas = json[0]["horas"].as_array().unwrap();
        assert_eq!(horas.len(), 6);
    }

    #[tokio::test]
    async fn historico_endpoint_clampa_horas_absurdas() {
        // Regressão do achado 2: `?horas=100000000` não pode virar uma
        // alocação gigante — o endpoint deve clampar no teto
        // (`HORAS_HISTORICO_MAX` = 24*30) em vez de repassar o valor cru.
        let estado = estado_teste();
        {
            let conn = estado.db.lock().unwrap();
            let agora = nucleo::coletor::agora_unix();
            conn.execute(
                "INSERT INTO containers (name, status, last_collected_at, uptime, criado_em)
                 VALUES ('app', 'running', ?1, 'Up 1 day', '')",
                rusqlite::params![agora],
            )
            .unwrap();
        }

        let (status, json) = get_json(
            criar_rotas(estado),
            "/api/containers/historico?horas=100000000",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let horas = json[0]["horas"].as_array().unwrap();
        assert_eq!(horas.len(), (24 * 30) as usize);
    }

    #[tokio::test]
    async fn historico_endpoint_clampa_horas_zero_ou_negativas() {
        let estado = estado_teste();
        {
            let conn = estado.db.lock().unwrap();
            let agora = nucleo::coletor::agora_unix();
            conn.execute(
                "INSERT INTO containers (name, status, last_collected_at, uptime, criado_em)
                 VALUES ('app', 'running', ?1, 'Up 1 day', '')",
                rusqlite::params![agora],
            )
            .unwrap();
        }

        let (status, json) =
            get_json(criar_rotas(estado), "/api/containers/historico?horas=-5").await;
        assert_eq!(status, StatusCode::OK);
        let horas = json[0]["horas"].as_array().unwrap();
        assert_eq!(horas.len(), 1);
    }

    #[tokio::test]
    async fn ia_custos_sem_banco_devolve_vazio() {
        // Força um caminho inexistente via env: a função `caminho_db_opencode`
        // lê `DEV_CLI_OPENCODE_DB` antes de cair no default de `~/.local/...`
        // (que pode existir no ambiente do dev).
        // `tempdir` seria ideal mas ia trazer uma dep nova; um path /tmp
        // com nome aleatório improvável basta.
        let caminho_falso = std::env::temp_dir().join("dev-cli-ia-test-que-nao-existe.db");
        // `set_var` é `unsafe` desde o Rust 1.86 (race em programas multi-thread);
        // aceitável aqui porque o teste é serial e o `unsafe` é local.
        unsafe { std::env::set_var("DEV_CLI_OPENCODE_DB", &caminho_falso) };

        let (status, json) = get_json(criar_rotas(estado_teste()), "/api/ia/custos").await;
        unsafe { std::env::remove_var("DEV_CLI_OPENCODE_DB") };

        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["disponivel"], false);
        assert_eq!(json["tokens"], 0);
        assert_eq!(json["modelos"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn ia_cambio_devolve_taxa() {
        // Busca ao vivo (rede pode estar indisponível no ambiente de
        // teste/CI) OU o fallback — o endpoint nunca deve falhar, então só
        // travamos que a taxa devolvida é positiva, sem prender o teste a
        // um valor específico (a taxa real muda todo dia).
        let (status, json) = get_json(criar_rotas(estado_teste()), "/api/ia/cambio").await;
        assert_eq!(status, StatusCode::OK);
        assert!(json["usd_brl"].as_f64().unwrap() > 0.0);
    }
}
