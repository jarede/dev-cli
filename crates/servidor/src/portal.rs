// Portal embutido: os arquivos de `web/dist` entram no binário em
// compile-time via `include_dir!` — depois de instalado, `dev-server`
// serve API e frontend sozinho, sem nenhum arquivo externo.
//
// O `#[cfg(portal_embutido)]` vem do build.rs: com `web/dist` presente no
// compile, o bloco embutido é usado; sem ele (ex.: `cargo build` sem ter
// rodado `npm run build`), cai na página explicativa — o build nunca quebra
// por falta do frontend.
// docs: https://docs.rs/include_dir/latest/include_dir/

use axum::Router;
use axum::http::Uri;
#[cfg(portal_embutido)]
use axum::http::{StatusCode, header};
#[cfg(not(portal_embutido))]
use axum::response::Html;
use axum::response::{IntoResponse, Response};
use axum::routing::get;

/// Router com um único fallback: o portal (embutido ou explicativo).
/// Isolado num router próprio para o main.rs "pendurar" só quando não há
/// `portal_dir` configurado, e para os testes montarem sem o resto da API.
pub fn rotas_portal() -> Router {
    Router::new().fallback(get(servir))
}

/// Texto para o log de subida do main.rs.
pub fn descricao() -> &'static str {
    if cfg!(portal_embutido) {
        "embutido no binário"
    } else {
        "ausente (build sem web/dist — rode `cd web && npm run build` e recompile, ou configure servidor.portal_dir)"
    }
}

#[cfg(portal_embutido)]
static DIST: include_dir::Dir<'_> = include_dir::include_dir!("$CARGO_MANIFEST_DIR/../../web/dist");

/// Content-Type pela extensão. Cobrimos só o que o build do Vite gera —
/// um `match` explícito é mais didático (e auditável) que uma dependência
/// de adivinhação de MIME para meia dúzia de casos.
/// docs: https://developer.mozilla.org/docs/Web/HTTP/Basics_of_HTTP/MIME_types
#[cfg(portal_embutido)]
fn content_type(caminho: &str) -> &'static str {
    match caminho.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(portal_embutido)]
async fn servir(uri: Uri) -> Response {
    let caminho = uri.path().trim_start_matches('/');

    // Arquivo real do dist (ex.: /assets/index-abc.js)?
    if let Some(arquivo) = DIST.get_file(caminho) {
        return (
            [(header::CONTENT_TYPE, content_type(caminho))],
            arquivo.contents(),
        )
            .into_response();
    }

    // Caminho com extensão que NÃO existe no dist = 404 de verdade (um
    // asset quebrado deve falhar alto, não devolver HTML disfarçado).
    // Sem extensão = rota do react-router (/historico, /testes...) — a SPA
    // resolve no cliente, então devolvemos o index.html (mesmo fallback do
    // ServeDir::not_found_service usado no modo portal_dir).
    let ultimo_segmento = caminho.rsplit('/').next().unwrap_or("");
    if ultimo_segmento.contains('.') {
        return StatusCode::NOT_FOUND.into_response();
    }
    match DIST.get_file("index.html") {
        Some(index) => (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            index.contents(),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Sem dist no compile: qualquer rota devolve a página explicativa —
/// honesta sobre o estado do binário, com o passo-a-passo de correção.
#[cfg(not(portal_embutido))]
async fn servir(_uri: Uri) -> Response {
    Html(
        "<!doctype html><meta charset=\"utf-8\"><title>dev-cli · portal</title>\
         <body style=\"font-family:serif;max-width:40rem;margin:4rem auto;line-height:1.6\">\
         <h1>portal não compilado neste binário</h1>\
         <p>Este build do <code>dev-server</code> foi feito sem o frontend. \
         Rode <code>cd web && npm run build</code> e recompile, ou configure \
         <code>servidor.portal_dir</code> apontando para um build do portal. \
         A API continua no ar em <code>/api/*</code>.</p>",
    )
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn get(caminho: &str) -> (StatusCode, String, String) {
        let resposta = rotas_portal()
            .oneshot(Request::builder().uri(caminho).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resposta.status();
        let content_type = resposta
            .headers()
            .get("content-type")
            .map(|v| v.to_str().unwrap_or("").to_string())
            .unwrap_or_default();
        let corpo = axum::body::to_bytes(resposta.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            content_type,
            String::from_utf8_lossy(&corpo).into_owned(),
        )
    }

    // Com o dist embutido (o caso do binário de release e do dev local
    // depois de `npm run build`): raiz e rotas SPA devolvem o index.html.
    #[cfg(portal_embutido)]
    #[tokio::test]
    async fn raiz_e_rota_spa_devolvem_index_html() {
        let (status, ct, corpo) = get("/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(ct.starts_with("text/html"));
        assert!(corpo.contains("<div id=\"root\">"));

        // Rota que só existe no react-router: fallback SPA para o index.
        let (status, ct, _) = get("/historico").await;
        assert_eq!(status, StatusCode::OK);
        assert!(ct.starts_with("text/html"));
    }

    #[cfg(portal_embutido)]
    #[tokio::test]
    async fn asset_inexistente_com_extensao_devolve_404() {
        let (status, _, _) = get("/assets/nao-existe.js").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    // Sem o dist no compile: página mínima explicativa em qualquer rota.
    #[cfg(not(portal_embutido))]
    #[tokio::test]
    async fn sem_dist_serve_pagina_explicativa() {
        let (status, ct, corpo) = get("/").await;
        assert_eq!(status, StatusCode::OK);
        assert!(ct.starts_with("text/html"));
        assert!(corpo.contains("portal não compilado"));
    }
}
