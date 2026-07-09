// Entry point do dev-server — o "dashboard sem tela" do dev-cli:
// a MESMA thread coletora do `logs dashboard`, mas em vez de uma TUI quem
// consome os dados é a API JSON (e, na Fase 3, o portal React).
//
// Duas "camadas de execução" convivem aqui:
//   - a thread coletora do nucleo é uma thread do SO (std::thread), síncrona;
//   - a API axum roda no runtime async do tokio.
// Elas não se falam diretamente: as duas escrevem/leem o MESMO arquivo
// SQLite (modo WAL), cada uma com a sua Connection.

mod api;

use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use clap::Parser;
use rusqlite::Connection;

use nucleo::coletor::{iniciar_coletor, ComandoColetor, EventoColeta, ParametrosColetor};
use nucleo::config::Config;
use nucleo::db::init_db;
use nucleo::executor::Executor;

/// Flags do dev-server. Mesma precedência do dev-cli:
/// flags > env DEV_CLI_* > arquivo TOML > defaults.
#[derive(Parser, Debug)]
#[command(
    name = "dev-server",
    version,
    about = "Servidor do dev-cli: coleta contínua de logs de containers + API JSON"
)]
struct Cli {
    /// Caminho do arquivo de configuração TOML.
    /// (default: ~/.config/dev-cli/config.toml, se existir)
    #[arg(long)]
    config: Option<PathBuf>,
    /// Host SSH ("user@host") para coletar de uma VM remota.
    /// Sem esta flag, executa `docker` localmente (modo padrão na VM).
    #[arg(long)]
    ssh: Option<String>,
    /// Caminho do banco SQLite (sobrepõe config/env).
    #[arg(long)]
    db: Option<PathBuf>,
    /// Endereço de escuta da API, "host:porta" (sobrepõe config/env).
    #[arg(long)]
    bind: Option<String>,
    /// Segundos entre coletas (sobrepõe config/env).
    #[arg(long)]
    intervalo: Option<u64>,
    /// Diretório com o build do portal web (web/dist) para servir como
    /// estático (sobrepõe config/env). Vazio/ausente = só a API.
    #[arg(long)]
    portal_dir: Option<PathBuf>,
}

// `#[tokio::main]`: cria o runtime tokio e roda o futuro até o fim.
// docs: https://docs.rs/tokio/latest/tokio/attr.main.html
#[tokio::main]
async fn main() {
    // Mesmo padrão do dev-cli: erro via `Display` no stderr, exit 1.
    if let Err(erro) = executar().await {
        eprintln!("{erro}");
        std::process::exit(1);
    }
}

async fn executar() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Config com precedência flags > env > arquivo > defaults —
    // exatamente como o `logs dashboard` do CLI.
    let cli = Cli::parse();
    let mut config = Config::carregar(cli.config.as_deref())?;
    if let Some(ssh) = &cli.ssh {
        config.coleta.ssh = ssh.clone();
    }
    if let Some(db) = &cli.db {
        config.coleta.db = db.display().to_string();
    }
    if let Some(bind) = &cli.bind {
        config.servidor.bind = bind.clone();
    }
    if let Some(intervalo) = cli.intervalo {
        config.coleta.intervalo_seg = intervalo;
    }
    if let Some(portal) = &cli.portal_dir {
        config.servidor.portal_dir = portal.display().to_string();
    }

    let executor = if config.coleta.ssh.is_empty() {
        Executor::Local
    } else {
        Executor::Ssh(config.coleta.ssh.clone())
    };

    // 2. Banco: diretório + schema ANTES de subir coletor e API (as duas
    // conexões apontam para o mesmo arquivo).
    let caminho_db = config.caminho_db();
    if let Some(pai) = caminho_db.parent() {
        std::fs::create_dir_all(pai)?;
    }
    let conn = Connection::open(&caminho_db)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    init_db(&conn)?;

    // 3. Sobe a thread coletora (a mesma do dashboard TUI).
    let (tx_eventos, rx_eventos) = mpsc::channel();
    let (handle_coletor, tx_comandos) = iniciar_coletor(
        ParametrosColetor {
            executor,
            db: caminho_db,
            intervalo: Duration::from_secs(config.coleta.intervalo_seg),
            tail_inicial: config.coleta.tail_inicial,
            retencao_horas: config.coleta.retencao_horas,
        },
        tx_eventos,
    );

    // 4. Dreno de eventos: sem TUI, só registramos falhas no stderr (o
    // journald captura). Precisa existir: um canal mpsc sem receptor vivo
    // acumularia mensagens para sempre. A thread termina sozinha quando o
    // coletor morre (o sender é dropado e o `recv` devolve `Err`).
    // docs: https://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html#method.recv
    let dreno = std::thread::spawn(move || {
        while let Ok(evento) = rx_eventos.recv() {
            if let EventoColeta::Falha(mensagem) = evento {
                eprintln!("coleta falhou: {mensagem}");
            }
        }
    });

    // 5. API: CORS permissivo para o portal da Fase 3 poder chamar de outra
    // origem (localhost:5173 do Vite, por exemplo). A API só expõe leitura,
    // então liberar origens é aceitável; restringir vira config no futuro.
    // docs: https://docs.rs/tower-http/latest/tower_http/cors/index.html
    let estado = api::EstadoApi {
        db: Arc::new(Mutex::new(conn)),
        config: Arc::new(config.clone()),
    };
    let mut rotas = api::criar_rotas(estado).layer(tower_http::cors::CorsLayer::permissive());
    if !config.servidor.portal_dir.is_empty() {
        rotas = rotas.fallback_service(tower_http::services::ServeDir::new(
            &config.servidor.portal_dir,
        ));
        println!("portal estático: {}", config.servidor.portal_dir);
    }

    let listener = tokio::net::TcpListener::bind(&config.servidor.bind).await?;
    println!("dev-server ouvindo em http://{}", config.servidor.bind);
    // `with_graceful_shutdown`: quando o futuro passado resolver (Ctrl-C ou
    // SIGTERM do systemd via ctrl_c/signal), o axum para de aceitar novas
    // conexões, termina as em andamento e o `.await` retorna.
    // docs: https://docs.rs/axum/latest/axum/serve/struct.Serve.html#method.with_graceful_shutdown
    axum::serve(listener, rotas)
        .with_graceful_shutdown(aguardar_sinal())
        .await?;

    // 6. Encerramento limpo, na ordem: pede para o coletor parar, espera a
    // thread dele, e o dreno cai junto (sender dropado).
    let _ = tx_comandos.send(ComandoColetor::Encerrar);
    let _ = handle_coletor.join();
    let _ = dreno.join();
    println!("dev-server encerrado");
    Ok(())
}

/// Resolve quando chega SIGINT (Ctrl-C) ou SIGTERM (o `systemctl stop`
/// manda SIGTERM) — o gatilho do graceful shutdown.
// `tokio::select!`: espera VÁRIOS futuros e segue com o primeiro que
// completar, descartando os outros.
// docs: https://docs.rs/tokio/latest/tokio/macro.select.html
async fn aguardar_sinal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let sigterm = async {
        // `signal` pode falhar ao registrar o handler; nesse caso ficamos
        // só com o Ctrl-C (pending() nunca resolve).
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sinal) => {
                sinal.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let sigterm = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = sigterm => {},
    }
}
