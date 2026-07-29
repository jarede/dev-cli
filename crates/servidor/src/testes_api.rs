// API HTTP das suítes de teste e2e. Persiste as suítes em
// `/etc/dev-cli/testes/<id>.toml` (caminho configurável) e mantém as
// execuções EM MEMÓRIA (um `Mutex<HashMap>` no `EstadoApi`) — execuções
// que estavam rodando somem se o dev-server reiniciar, o que é
// aceitável: a próxima vez a suíte for rodada, o cliente recebe uma nova
// `id_execucao`.
//
// Endpoints:
//   GET    /api/testes/suites                       → Vec<Suite>
//   GET    /api/testes/suites/{id}                  → Suite
//   POST   /api/testes/suites                       → Suite (cria/atualiza)
//   DELETE /api/testes/suites/{id}                  → 204
//   POST   /api/testes/suites/{id}/rodar            → { id_execucao }
//   GET    /api/testes/suites/{id}/execucoes        → Vec<Execucao> (histórico, mais recente primeiro)
//   GET    /api/testes/execucoes/{id}               → Execucao
//   POST   /api/testes/execucoes/{id}/cancelar      → 204
//
// Os handlers usam o MESMO `EstadoApi` do `api.rs` (em vez de um
// `EstadoTestes` separado) — assim, o sub-router pode ser `.merge`-ado
// no router principal sem a fricção de tipos de estado diferentes
// (Router<EstadoApi>.merge exige Router<EstadoApi>).
//
// Sobre IO: as leituras do diretório e dos TOML são pequenas (poucos
// arquivos pequenos) — chamá-las diretamente é mais simples e o bloqueio
// do runtime é desprezível. A complicação extra de `spawn_blocking` (que
// exige erros `Send`) não vale a pena.
//
// docs: docs/design_handoff_portal_dev_cli/README.md (seção 5)

use std::collections::HashMap;

use axum::extract::{Path, Query, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use nucleo::coletor::agora_unix;
use nucleo::testes as nuc_testes;
use serde::{Deserialize, Serialize};

use crate::api::EstadoApi;

/// Resposta de `POST /api/testes/suites/{id}/rodar` — o cliente abre
/// polling em `GET /api/testes/execucoes/{id_execucao}` em seguida.
#[derive(Serialize)]
struct InicioExecucao {
    id_execucao: String,
}

/// Cabeçalho exigido em TODA chamada às rotas de testes. Não é segredo (o
/// nome está aqui, no código-fonte aberto) — o que importa é que ele NÃO É
/// UM CABEÇALHO "SIMPLES" do fetch/CORS. `main.rs` não aplica CORS
/// permissivo neste sub-router; então, para o navegador mandar uma
/// requisição com um cabeçalho custom, ele PRECISA primeiro mandar um
/// preflight (`OPTIONS`) — que falha aqui por falta de
/// `Access-Control-Allow-Origin`, e o navegador NUNCA chega a mandar a
/// requisição real.
///
/// Isso fecha uma lacuna que o CORS sozinho NÃO fecha: `POST /rodar` e
/// `POST /cancelar` não têm corpo, então são "requisições simples" pelo
/// critério do fetch — o navegador as manda de qualquer jeito, CSRF-style,
/// mesmo sem nenhum cabeçalho CORS na resposta (um `<form method="POST">`
/// em QUALQUER site aciona isso sem precisar de JavaScript, porque CORS só
/// controla se o JavaScript pode LER a resposta, não se o navegador pode
/// ENVIAR a requisição). Exigir este cabeçalho custom força o preflight
/// em TODA requisição a este router, inclusive nas que não têm corpo.
/// docs: https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS#simple_requests
/// docs: https://developer.mozilla.org/en-US/docs/Web/Security/Attacks/CSRF
const CABECALHO_ANTICSRF: &str = "x-dev-cli-portal";

/// Middleware: bloqueia (403) qualquer requisição sem `CABECALHO_ANTICSRF`.
/// Ver o comentário da constante para o raciocínio completo.
async fn exigir_cabecalho_anticsrf(req: Request, next: Next) -> Response {
    if !req.headers().contains_key(CABECALHO_ANTICSRF) {
        return (
            StatusCode::FORBIDDEN,
            "cabeçalho x-dev-cli-portal ausente — bloqueado (proteção anti-CSRF)",
        )
            .into_response();
    }
    next.run(req).await
}

/// Cria o sub-router com as rotas de testes. Estado = `EstadoApi` (o
/// mesmo do router principal — daí poder dar `.merge` direto). Todo o
/// router passa pelo middleware anti-CSRF: mesmo as rotas de leitura
/// (`GET /suites`, `GET /execucoes/{id}`) ficam atrás dele, por
/// simplicidade de ter uma única política para o sub-router inteiro.
pub fn criar_rotas_testes() -> Router<EstadoApi> {
    Router::new()
        .route(
            "/api/testes/suites",
            get(listar_suites_handler).post(criar_suite_handler),
        )
        .route(
            "/api/testes/suites/{id}",
            get(ler_suite_handler).delete(remover_suite_handler),
        )
        .route("/api/testes/suites/{id}/rodar", post(rodar_suite_handler))
        .route(
            "/api/testes/suites/{id}/execucoes",
            get(listar_execucoes_da_suite_handler),
        )
        .route("/api/testes/execucoes/{id}", get(ler_execucao_handler))
        .route(
            "/api/testes/execucoes/{id}/cancelar",
            post(cancelar_execucao_handler),
        )
        .layer(middleware::from_fn(exigir_cabecalho_anticsrf))
}

// ─── Handlers ─────────────────────────────────────────────────────────

async fn listar_suites_handler(
    State(estado): State<EstadoApi>,
) -> Result<Json<Vec<nuc_testes::Suite>>, (StatusCode, String)> {
    let suites = nuc_testes::listar_suites(&estado.testes_dir).map_err(erro_interno)?;
    Ok(Json(suites))
}

async fn ler_suite_handler(
    State(estado): State<EstadoApi>,
    Path(id): Path<String>,
) -> Result<Json<nuc_testes::Suite>, (StatusCode, String)> {
    let suite = nuc_testes::ler_suite(&estado.testes_dir, &id)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    Ok(Json(suite))
}

/// Body de `POST /api/testes/suites` — JSON com a mesma forma de
/// `Suite` (serde desserializa direto). A resposta devolve a suíte
/// gravada (com `id` validado).
async fn criar_suite_handler(
    State(estado): State<EstadoApi>,
    Json(suite): Json<nuc_testes::Suite>,
) -> Result<Json<nuc_testes::Suite>, (StatusCode, String)> {
    // `Suite::com_id` revalida o id — defesa em profundidade contra um
    // cliente malicioso/que esqueceu de validar.
    let suite = nuc_testes::Suite::com_id(
        &suite.id,
        suite.nome.clone(),
        suite.timeout_etapa_seg,
        suite.passos.clone(),
    )
    .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    nuc_testes::gravar_suite(&estado.testes_dir, &suite).map_err(erro_interno)?;
    Ok(Json(suite))
}

async fn remover_suite_handler(
    State(estado): State<EstadoApi>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    if !nuc_testes::id_valido(&id) {
        return Err((StatusCode::BAD_REQUEST, format!("id inválido: {id:?}")));
    }
    let caminho = estado.testes_dir.join(format!("{id}.toml"));
    let _ = std::fs::remove_file(caminho);
    // 204 mesmo se o arquivo não existia — DELETE é idempotente.
    Ok(StatusCode::NO_CONTENT)
}

/// Por quanto tempo uma execução JÁ TERMINADA (sucesso ou falha) fica
/// disponível para consulta antes de ser podada do registro em memória.
/// Sem isso, `estado.execucoes` só cresce — o dev-server é um serviço
/// systemd de longa duração, então em produção isso vaza memória
/// indefinidamente (uma entrada por `rodar`, nunca removida).
const RETENCAO_EXECUCOES_SEG: i64 = 24 * 60 * 60;

/// Remove do registro as execuções já terminadas e mais velhas que
/// `RETENCAO_EXECUCOES_SEG`. Chamado a cada novo `rodar` — mantém o mapa
/// limitado ao "recente" sem precisar de uma tarefa de limpeza separada.
/// Execuções ainda `Rodando` NUNCA são podadas aqui (mesmo que antigas —
/// isso indicaria uma pipeline travada, não um motivo para escondê-la).
fn podar_execucoes_antigas(execucoes: &mut HashMap<String, nuc_testes::Execucao>, agora_unix: i64) {
    execucoes.retain(|_, exec| {
        exec.conclusao == nuc_testes::ConclusaoExecucao::Rodando
            || agora_unix - exec.iniciada_em_unix < RETENCAO_EXECUCOES_SEG
    });
}

async fn rodar_suite_handler(
    State(estado): State<EstadoApi>,
    Path(id): Path<String>,
) -> Result<Json<InicioExecucao>, (StatusCode, String)> {
    // Lê a suíte do disco (não confiamos em cache — se alguém editou o
    // TOML, a próxima execução pega a versão atual).
    let suite = nuc_testes::ler_suite(&estado.testes_dir, &id)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;

    // Gera um ID único (timestamp nanos + contador atômico + PID evita
    // colisão em execuções quase simultâneas).
    let id_execucao = format!("exec-{}-{}", agora_unix(), rand_suffix());
    let execucao = nuc_testes::Execucao::nova(id_execucao.clone(), &suite, agora_unix());

    // Insere no registro de execuções ANTES de spawnar o runner: assim,
    // se o cliente pollar logo após receber a resposta, já encontra.
    // Aproveita o lock para podar entradas velhas (ver `podar_execucoes_antigas`).
    {
        let mut g = estado.execucoes.lock().map_err(erro_interno)?;
        podar_execucoes_antigas(&mut g, agora_unix());
        g.insert(id_execucao.clone(), execucao.clone());
    }

    // Spawna o runner. O callback atualiza o registro em memória a cada
    // mudança — o cliente (portal) vê o progresso via polling.
    let progresso: nuc_testes::ProgressoCallback = {
        let execucoes = estado.execucoes.clone();
        let id_exec = id_execucao.clone();
        std::sync::Arc::new(move |nova: &nuc_testes::Execucao| {
            if let Ok(mut g) = execucoes.lock() {
                g.insert(id_exec.clone(), nova.clone());
            }
        })
    };
    let execucoes_handle = estado.execucoes.clone();
    let id_exec_spawn = id_execucao.clone();
    tokio::spawn(async move {
        let final_exec = nuc_testes::rodar(execucao, &suite, progresso).await;
        // Garante que o estado final ficou registrado (defesa contra
        // qualquer caminho de saída que tenha pulado um callback).
        if let Ok(mut g) = execucoes_handle.lock() {
            g.insert(id_exec_spawn, final_exec);
        }
    });

    Ok(Json(InicioExecucao { id_execucao }))
}

/// Query string de `GET /suites/{id}/execucoes`: `?limite=N` (default 20,
/// máx 200 — mesmo padrão de `buscarAlertas`/`buscarErros` no portal).
#[derive(Deserialize)]
struct ListarExecucoesParams {
    limite: Option<usize>,
}

/// Histórico de execuções de UMA suíte, mais recente primeiro. Filtra o
/// registro em memória por `suite_id` — não existe índice separado por
/// suíte porque o volume (algumas dezenas de execuções por suíte, podadas
/// após `RETENCAO_EXECUCOES_SEG`) não justifica a complexidade extra.
async fn listar_execucoes_da_suite_handler(
    State(estado): State<EstadoApi>,
    Path(id): Path<String>,
    Query(params): Query<ListarExecucoesParams>,
) -> Result<Json<Vec<nuc_testes::Execucao>>, (StatusCode, String)> {
    let limite = params.limite.unwrap_or(20).clamp(1, 200);
    let g = estado.execucoes.lock().map_err(erro_interno)?;
    let mut execucoes: Vec<nuc_testes::Execucao> = g
        .values()
        .filter(|exec| exec.suite_id == id)
        .cloned()
        .collect();
    // Mais recente primeiro: `iniciada_em_unix` decrescente.
    execucoes.sort_by_key(|exec| std::cmp::Reverse(exec.iniciada_em_unix));
    execucoes.truncate(limite);
    Ok(Json(execucoes))
}

async fn ler_execucao_handler(
    State(estado): State<EstadoApi>,
    Path(id): Path<String>,
) -> Result<Json<nuc_testes::Execucao>, (StatusCode, String)> {
    let g = estado.execucoes.lock().map_err(erro_interno)?;
    let exec = g.get(&id).cloned().ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            format!("execução {id:?} não encontrada"),
        )
    })?;
    Ok(Json(exec))
}

async fn cancelar_execucao_handler(
    State(estado): State<EstadoApi>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    // Primeiro confirma que a execução existe — sem isso, "cancelar" um
    // ID inexistente viraria 204 (sucesso) e o cliente acharia que
    // parou algo.
    {
        let g = estado.execucoes.lock().map_err(erro_interno)?;
        if !g.contains_key(&id) {
            return Err((
                StatusCode::NOT_FOUND,
                format!("execução {id:?} não encontrada"),
            ));
        }
    }
    nuc_testes::cancelar(&id);
    Ok(StatusCode::NO_CONTENT)
}

// ─── Utilitários ──────────────────────────────────────────────────────

fn erro_interno<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Sufixo aleatório curto (≤6 chars hex) — só precisa reduzir a chance
/// de colisão entre execuções iniciadas no mesmo segundo. Sem `rand`:
/// `nanos * 31 + contador + pid` é o suficiente (não é criptográfico, é
/// só desempate). Mantém-se curto para a UI mostrar um ID legível.
fn rand_suffix() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};
    static CONTADOR: AtomicU32 = AtomicU32::new(0);
    let n = CONTADOR.fetch_add(1, Ordering::Relaxed) as u64;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let pid = std::process::id() as u64;
    let misturado = nanos.wrapping_mul(31).wrapping_add(n).wrapping_add(pid);
    let hex = format!("{misturado:x}");
    let len = hex.len();
    if len <= 6 {
        hex
    } else {
        hex[len - 6..].to_string()
    }
}

/// Re-export do helper do nucleo pra evitar duplicar lógica no `main.rs`.
pub use nuc_testes::diretorio_testes;

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use nucleo::config::Config;
    use nucleo::db::init_db;
    use rusqlite::Connection;

    /// Estado de teste com diretório de suítes em tempdir (criado
    /// zerado a cada teste) e mapa de execuções em memória.
    fn estado_teste() -> (EstadoApi, PathBuf) {
        let dir = tempdir();
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        let estado = EstadoApi {
            db: Arc::new(Mutex::new(conn)),
            config: Arc::new(Config::default()),
            testes_dir: dir.clone(),
            execucoes: Arc::new(Mutex::new(HashMap::new())),
        };
        (estado, dir)
    }

    fn tempdir() -> PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("dev-cli-testes-api-{pid}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Helper: GET + parse JSON. O router já tem `with_state` aplicado
    /// (estado injetado), então o tipo do Router é `Router<()>`. Inclui o
    /// cabeçalho anti-CSRF (ver `CABECALHO_ANTICSRF`) — sem ele, todo
    /// request cai em 403 pelo middleware do router.
    async fn get_json(rotas: Router<()>, uri: &str) -> (StatusCode, serde_json::Value) {
        let resposta = rotas
            .oneshot(
                Request::builder()
                    .uri(uri)
                    .header(CABECALHO_ANTICSRF, "1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resposta.status();
        let corpo = resposta.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&corpo).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    /// Helper: POST com body JSON e parse da resposta (mesmo cabeçalho
    /// anti-CSRF de `get_json`).
    async fn post_json(
        rotas: Router<()>,
        uri: &str,
        body: serde_json::Value,
    ) -> (StatusCode, serde_json::Value) {
        let resposta = rotas
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .header(CABECALHO_ANTICSRF, "1")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resposta.status();
        let corpo = resposta.into_body().collect().await.unwrap().to_bytes();
        let json = serde_json::from_slice(&corpo).unwrap_or(serde_json::Value::Null);
        (status, json)
    }

    fn suite_exemplo(id: &str) -> nuc_testes::Suite {
        nuc_testes::Suite {
            id: id.to_string(),
            nome: format!("suíte {id}"),
            timeout_etapa_seg: 10,
            passos: vec![
                nuc_testes::Passo {
                    tipo: nuc_testes::TipoPasso::Exec,
                    cmd: "echo oi".to_string(),
                    esperado: None,
                },
                nuc_testes::Passo {
                    tipo: nuc_testes::TipoPasso::Valida,
                    cmd: "echo oi".to_string(),
                    esperado: Some("oi".to_string()),
                },
            ],
        }
    }

    #[tokio::test]
    async fn listar_vazio_quando_diretorio_vazio() {
        let (estado, _dir) = estado_teste();
        let (status, json) = get_json(
            criar_rotas_testes().with_state(estado),
            "/api/testes/suites",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn criar_listar_ler_roundtrip() {
        let (estado, _dir) = estado_teste();
        let rotas = criar_rotas_testes().with_state(estado);

        // Criar.
        let (status, _) = post_json(
            rotas.clone(),
            "/api/testes/suites",
            serde_json::to_value(suite_exemplo("rt")).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        // Listar: 1 entrada.
        let (_, json) = get_json(rotas.clone(), "/api/testes/suites").await;
        let lista = json.as_array().unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0]["id"], "rt");

        // Ler específica.
        let (status, json) = get_json(rotas.clone(), "/api/testes/suites/rt").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["id"], "rt");
        assert_eq!(json["passos"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn criar_com_id_invalido_retorna_400() {
        let (estado, _dir) = estado_teste();
        let rotas = criar_rotas_testes().with_state(estado);
        let mut s = suite_exemplo("ok");
        s.id = "../etc".to_string();
        let (status, _) = post_json(
            rotas,
            "/api/testes/suites",
            serde_json::to_value(s).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn ler_suite_inexistente_retorna_404() {
        let (estado, _dir) = estado_teste();
        let (status, _) = get_json(
            criar_rotas_testes().with_state(estado),
            "/api/testes/suites/nope",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn delete_remove_suite() {
        let (estado, dir) = estado_teste();
        nuc_testes::gravar_suite(&dir, &suite_exemplo("del")).unwrap();
        let rotas = criar_rotas_testes().with_state(estado);

        // DELETE.
        let resposta = rotas
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/testes/suites/del")
                    .header(CABECALHO_ANTICSRF, "1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resposta.status(), StatusCode::NO_CONTENT);

        // GET subsequente: 404.
        let (status, _) = get_json(rotas, "/api/testes/suites/del").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rodar_suite_retorna_id_execucao_e_poll_funciona() {
        let (estado, dir) = estado_teste();
        nuc_testes::gravar_suite(&dir, &suite_exemplo("run")).unwrap();
        let rotas = criar_rotas_testes().with_state(estado);

        // Dispara a execução.
        let (status, json) = post_json(
            rotas.clone(),
            "/api/testes/suites/run/rodar",
            serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let id_exec = json["id_execucao"].as_str().unwrap().to_string();
        assert!(id_exec.starts_with("exec-"));

        // Polling até terminar (timeout generoso: 5s). A suíte usa
        // `echo oi` (instantâneo) e tem timeout 10s/etapa.
        let mut conclusao = None;
        for _ in 0..50 {
            let (status, json) =
                get_json(rotas.clone(), &format!("/api/testes/execucoes/{id_exec}")).await;
            assert_eq!(status, StatusCode::OK);
            let c = json["conclusao"].as_str().unwrap();
            if c != "rodando" {
                conclusao = Some(c.to_string());
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(conclusao.as_deref(), Some("sucesso"));
    }

    #[tokio::test]
    async fn historico_de_execucoes_lista_mais_recente_primeiro_e_filtra_por_suite() {
        let (estado, dir) = estado_teste();
        nuc_testes::gravar_suite(&dir, &suite_exemplo("hist-a")).unwrap();
        nuc_testes::gravar_suite(&dir, &suite_exemplo("hist-b")).unwrap();
        let rotas = criar_rotas_testes().with_state(estado);

        // Roda "hist-a" duas vezes e "hist-b" uma vez — o histórico de
        // "hist-a" não pode incluir a execução de "hist-b".
        for _ in 0..2 {
            let (_, json) = post_json(
                rotas.clone(),
                "/api/testes/suites/hist-a/rodar",
                serde_json::json!({}),
            )
            .await;
            let id_exec = json["id_execucao"].as_str().unwrap().to_string();
            aguardar_conclusao(&rotas, &id_exec).await;
        }
        let (_, json) = post_json(
            rotas.clone(),
            "/api/testes/suites/hist-b/rodar",
            serde_json::json!({}),
        )
        .await;
        let id_exec_b = json["id_execucao"].as_str().unwrap().to_string();
        aguardar_conclusao(&rotas, &id_exec_b).await;

        let (status, json) = get_json(rotas.clone(), "/api/testes/suites/hist-a/execucoes").await;
        assert_eq!(status, StatusCode::OK);
        let lista = json.as_array().unwrap();
        assert_eq!(
            lista.len(),
            2,
            "só as 2 execuções de hist-a, não a de hist-b"
        );
        assert!(lista.iter().all(|e| e["suite_id"] == "hist-a"));
        // Mais recente primeiro: `iniciada_em_unix` não-crescente.
        let t0 = lista[0]["iniciada_em_unix"].as_i64().unwrap();
        let t1 = lista[1]["iniciada_em_unix"].as_i64().unwrap();
        assert!(t0 >= t1);
    }

    #[tokio::test]
    async fn historico_de_execucoes_respeita_limite() {
        let (estado, dir) = estado_teste();
        nuc_testes::gravar_suite(&dir, &suite_exemplo("lim")).unwrap();
        let rotas = criar_rotas_testes().with_state(estado);

        for _ in 0..3 {
            let (_, json) = post_json(
                rotas.clone(),
                "/api/testes/suites/lim/rodar",
                serde_json::json!({}),
            )
            .await;
            let id_exec = json["id_execucao"].as_str().unwrap().to_string();
            aguardar_conclusao(&rotas, &id_exec).await;
        }

        let (_, json) = get_json(rotas.clone(), "/api/testes/suites/lim/execucoes?limite=2").await;
        assert_eq!(json.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn historico_de_execucoes_suite_sem_execucoes_devolve_vazio() {
        let (estado, dir) = estado_teste();
        nuc_testes::gravar_suite(&dir, &suite_exemplo("nunca-rodou")).unwrap();
        let rotas = criar_rotas_testes().with_state(estado);

        let (status, json) = get_json(rotas, "/api/testes/suites/nunca-rodou/execucoes").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json.as_array().unwrap().len(), 0);
    }

    /// Espera uma execução sair de "rodando" (timeout generoso: 5s — as
    /// suítes de teste usam `echo`, instantâneo).
    async fn aguardar_conclusao(rotas: &Router<()>, id_exec: &str) {
        for _ in 0..50 {
            let (_, json) =
                get_json(rotas.clone(), &format!("/api/testes/execucoes/{id_exec}")).await;
            if json["conclusao"].as_str().unwrap() != "rodando" {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        panic!("execução {id_exec} não concluiu a tempo");
    }

    #[tokio::test]
    async fn rodar_suite_com_falha_exec_e_marcada_como_falha() {
        let (estado, dir) = estado_teste();
        // Suíte que sempre falha.
        let s = nuc_testes::Suite {
            id: "fail".into(),
            nome: "fail".into(),
            timeout_etapa_seg: 5,
            passos: vec![nuc_testes::Passo {
                tipo: nuc_testes::TipoPasso::Exec,
                cmd: "false".into(),
                esperado: None,
            }],
        };
        nuc_testes::gravar_suite(&dir, &s).unwrap();
        let rotas = criar_rotas_testes().with_state(estado);

        let (_, json) = post_json(
            rotas.clone(),
            "/api/testes/suites/fail/rodar",
            serde_json::json!({}),
        )
        .await;
        let id_exec = json["id_execucao"].as_str().unwrap().to_string();

        let mut conclusao = None;
        for _ in 0..50 {
            let (_, json) =
                get_json(rotas.clone(), &format!("/api/testes/execucoes/{id_exec}")).await;
            let c = json["conclusao"].as_str().unwrap();
            if c != "rodando" {
                conclusao = Some(c.to_string());
                assert_eq!(json["estados"][0]["status"], "falha_exec");
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(conclusao.as_deref(), Some("falha"));
    }

    #[tokio::test]
    async fn cancelar_execucao_marca_pulado() {
        let (estado, dir) = estado_teste();
        // Suíte longa (várias etapas) — o cancelamento tem que
        // acontecer ANTES da última etapa terminar.
        let s = nuc_testes::Suite {
            id: "long".into(),
            nome: "long".into(),
            timeout_etapa_seg: 30,
            passos: vec![
                nuc_testes::Passo {
                    tipo: nuc_testes::TipoPasso::Exec,
                    #[cfg(unix)]
                    cmd: "sleep 1".into(),
                    #[cfg(windows)]
                    cmd: "ping -n 2 127.0.0.1 >nul".into(),
                    esperado: None,
                },
                nuc_testes::Passo {
                    tipo: nuc_testes::TipoPasso::Exec,
                    #[cfg(unix)]
                    cmd: "sleep 1".into(),
                    #[cfg(windows)]
                    cmd: "ping -n 2 127.0.0.1 >nul".into(),
                    esperado: None,
                },
                nuc_testes::Passo {
                    tipo: nuc_testes::TipoPasso::Exec,
                    #[cfg(unix)]
                    cmd: "sleep 1".into(),
                    #[cfg(windows)]
                    cmd: "ping -n 2 127.0.0.1 >nul".into(),
                    esperado: None,
                },
            ],
        };
        nuc_testes::gravar_suite(&dir, &s).unwrap();
        let rotas = criar_rotas_testes().with_state(estado);

        let (_, json) = post_json(
            rotas.clone(),
            "/api/testes/suites/long/rodar",
            serde_json::json!({}),
        )
        .await;
        let id_exec = json["id_execucao"].as_str().unwrap().to_string();

        // Espera a primeira etapa entrar em "rodando".
        let mut comecou = false;
        for _ in 0..50 {
            let (_, json) =
                get_json(rotas.clone(), &format!("/api/testes/execucoes/{id_exec}")).await;
            let estados = json["estados"].as_array().unwrap();
            if estados.iter().any(|e| e["status"] == "rodando") {
                comecou = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(comecou, "primeira etapa deveria ter começado");

        // Cancela.
        let resposta = rotas
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/testes/execucoes/{id_exec}/cancelar"))
                    .header(CABECALHO_ANTICSRF, "1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resposta.status(), StatusCode::NO_CONTENT);

        // Espera terminar — a etapa atual termina, as seguintes viram
        // "pulado", conclusão = "falha".
        let mut conclusao = None;
        for _ in 0..100 {
            let (_, json) =
                get_json(rotas.clone(), &format!("/api/testes/execucoes/{id_exec}")).await;
            let c = json["conclusao"].as_str().unwrap();
            if c != "rodando" {
                conclusao = Some(c.to_string());
                let estados = json["estados"].as_array().unwrap();
                let tem_pulado = estados.iter().any(|e| e["status"] == "pulado");
                assert!(
                    tem_pulado,
                    "esperava alguma etapa pulada, estados: {estados:?}"
                );
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        assert_eq!(conclusao.as_deref(), Some("falha"));
    }

    #[tokio::test]
    async fn cancelar_execucao_inexistente_retorna_404() {
        let (estado, _dir) = estado_teste();
        let rotas = criar_rotas_testes().with_state(estado);
        let resposta = rotas
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/testes/execucoes/inexistente/cancelar")
                    .header(CABECALHO_ANTICSRF, "1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resposta.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn ler_execucao_inexistente_retorna_404() {
        let (estado, _dir) = estado_teste();
        let (status, _) = get_json(
            criar_rotas_testes().with_state(estado),
            "/api/testes/execucoes/nope",
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn requisicao_sem_cabecalho_anticsrf_e_bloqueada() {
        // Regressão do achado de segurança: `POST /rodar` sem corpo é uma
        // "requisição simples" que o navegador manda mesmo sem CORS — o
        // middleware anti-CSRF é a defesa real contra isso, e este teste
        // garante que ele bloqueia ANTES de chegar no handler (que rodaria
        // um comando de verdade se não fosse bloqueado).
        let (estado, dir) = estado_teste();
        nuc_testes::gravar_suite(&dir, &suite_exemplo("bloqueada")).unwrap();
        let rotas = criar_rotas_testes().with_state(estado);

        let resposta = rotas
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/testes/suites/bloqueada/rodar")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resposta.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn rand_suffix_gera_strings_curtas() {
        // Garante o formato (≤6 chars) e variação entre chamadas.
        let a = rand_suffix();
        let b = rand_suffix();
        assert!(a.len() <= 6);
        assert!(b.len() <= 6);
    }
}
