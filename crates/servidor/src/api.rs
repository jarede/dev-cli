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

use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use rusqlite::Connection;
use serde::Serialize;

use nucleo::config::Config;

/// Estado compartilhado entre todos os handlers da API.
// `#[derive(Clone)]`: o axum clona o estado para cada handler; `Arc` faz o
// clone ser barato (só incrementa o contador de referências, não copia a
// conexão nem a config).
// docs: https://doc.rust-lang.org/std/sync/struct.Arc.html
#[derive(Clone)]
pub struct EstadoApi {
    #[allow(dead_code)] // usado a partir da Task 5
    pub db: Arc<Mutex<Connection>>,
    #[allow(dead_code)] // usado a partir da Task 5
    pub config: Arc<Config>,
}

/// Monta o `Router` com todas as rotas da API.
/// Separado do `main` para os testes montarem o mesmo router com um banco
/// em memória e chamarem os handlers SEM subir um servidor TCP.
pub fn criar_rotas(estado: EstadoApi) -> Router {
    Router::new()
        .route("/api/saude", get(saude))
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

/// Converte qualquer erro exibível numa resposta 500 com a mensagem no
/// corpo. Usado com `.map_err(erro_interno)?` nos handlers.
// Genérico em `E: Display` para aceitar `Box<dyn Error>`, `PoisonError`...
// docs: https://doc.rust-lang.org/std/fmt/trait.Display.html
#[allow(dead_code)] // usado a partir da Task 5
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
    async fn rota_desconhecida_e_404() {
        let (status, _) = get_json(criar_rotas(estado_teste()), "/nao-existe").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }
}
