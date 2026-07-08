// Entry point do dev-server. Nesta primeira versão só resolve a config e
// sobe a API; a Task 7 adiciona a thread coletora, CORS e graceful shutdown.

mod api;

use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use nucleo::config::Config;
use nucleo::db::init_db;

// `#[tokio::main]`: transforma o `async fn main` em um `fn main` que cria o
// runtime tokio e roda o futuro até o fim — axum é async e precisa disso.
// docs: https://docs.rs/tokio/latest/tokio/attr.main.html
#[tokio::main]
async fn main() {
    // Mesmo padrão do dev-cli: o erro sai via `Display` no stderr, exit 1.
    if let Err(erro) = executar().await {
        eprintln!("{erro}");
        std::process::exit(1);
    }
}

async fn executar() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::carregar(None)?;

    // Banco: garante diretório e schema antes de atender requests.
    let caminho_db = config.caminho_db();
    if let Some(pai) = caminho_db.parent() {
        std::fs::create_dir_all(pai)?;
    }
    let conn = Connection::open(&caminho_db)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    init_db(&conn)?;

    let bind = config.servidor.bind.clone();
    let estado = api::EstadoApi {
        db: Arc::new(Mutex::new(conn)),
        config: Arc::new(config),
    };
    let rotas = api::criar_rotas(estado);

    let listener = tokio::net::TcpListener::bind(&bind).await?;
    println!("dev-server ouvindo em http://{bind}");
    axum::serve(listener, rotas).await?;
    Ok(())
}
