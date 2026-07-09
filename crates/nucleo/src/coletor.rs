// CASCA DE IO: o coletor de logs — o coração "ao vivo" do dashboard.
//
// Duas peças:
//   - `coletar_ciclo`: UM ciclo completo de coleta (docker ps -> alertas ->
//     docker logs incremental -> parse -> SQLite -> prune). Reutilizável:
//     o CLI roda em thread; o futuro dev-server (Fase 2) roda como serviço.
//   - `iniciar_coletor`: sobe a thread que repete o ciclo a cada intervalo
//     e conversa com a TUI por DOIS canais mpsc (eventos para lá, comandos
//     para cá).
//
// Sobre concorrência: `rusqlite::Connection` não é `Sync` (não pode ser
// compartilhada entre threads), então a thread coletora abre a SUA conexão
// e a TUI usa outra. O modo WAL do SQLite permite um escritor e vários
// leitores simultâneos sem bloqueio — exatamente o nosso caso.
// docs: https://www.sqlite.org/wal.html
// docs: https://doc.rust-lang.org/book/ch16-00-concurrency.html

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

use crate::core::{categorizar_por_nivel, parse_loguru_line};
use crate::db::{
    armazenar_contagens, armazenar_linhas, armazenar_requests, init_db, prune_antigos,
    verificar_status_containers,
};
use crate::executor::{Executor, listar_containers, obter_logs, obter_logs_desde};

/// O que a thread coletora anuncia para quem estiver ouvindo (a TUI).
#[derive(Debug)]
pub enum EventoColeta {
    /// Um ciclo terminou com sucesso; há dados novos no banco.
    Novo,
    /// O ciclo falhou (docker/ssh fora do ar etc.); tenta de novo no próximo.
    Falha(String),
}

/// O que quem estiver de fora pode pedir à thread coletora.
#[derive(Debug)]
pub enum ComandoColetor {
    /// Executa um ciclo imediatamente (tecla `r` do dashboard).
    ColetarAgora,
    /// Termina a thread de forma limpa.
    Encerrar,
}

/// Parâmetros para subir o coletor (agrupados numa struct para a assinatura
/// de `iniciar_coletor` não virar uma fila de argumentos posicionais).
pub struct ParametrosColetor {
    pub executor: Executor,
    pub db: PathBuf,
    pub intervalo: Duration,
    pub tail_inicial: usize,
    pub retencao_horas: u64,
}

/// Timestamp Unix atual em segundos (0 se o relógio estiver antes de 1970).
/// Pública porque a API da Fase 2 usa o mesmo relógio para calcular o corte
/// da janela — evita duas definições de "agora" no workspace.
pub fn agora_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// UM ciclo de coleta. Não decide QUANDO rodar — só roda. Reutilizado pelo
/// CLI (em thread) e pelo futuro dev-server.
pub fn coletar_ciclo(
    executor: &Executor,
    conn: &Connection,
    tail_inicial: usize,
    retencao_horas: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let agora = agora_unix();

    // 1. Quem está rodando? (também detecta parados/reiniciados)
    let rodando = listar_containers(executor)?;
    let nomes: Vec<String> = rodando.iter().map(|c| c.nome.clone()).collect();
    // Os alertas ficam gravados na tabela `alerts`; a TUI lê o status
    // 'stopped' direto de `containers`, então aqui só registramos.
    let _ = verificar_status_containers(conn, &nomes, agora)?;

    // 2. Coleta incremental de cada container rodando.
    for c in &rodando {
        let ultima_coleta: i64 = conn
            .query_row(
                "SELECT COALESCE(last_collected_at, 0) FROM containers WHERE name = ?1",
                rusqlite::params![c.nome],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let conteudo = if ultima_coleta == 0 {
            // Primeira vez que vemos este container: pega o rabo do log.
            obter_logs(executor, &c.nome, tail_inicial)?
        } else {
            // Já conhecido: só o que chegou desde a última coleta.
            obter_logs_desde(executor, &c.nome, ultima_coleta)?
        };

        // 3. Parse: linhas por nível + requests HTTP (formato Loguru).
        let grupos = categorizar_por_nivel(&conteudo);
        let niveis: std::collections::BTreeMap<String, usize> =
            grupos.iter().map(|(k, v)| (k.clone(), v.len())).collect();
        let entradas: Vec<_> = conteudo.lines().filter_map(parse_loguru_line).collect();

        // 4. Persiste tudo desta coleta com o MESMO collected_at.
        armazenar_contagens(conn, &c.nome, &niveis, agora)?;
        armazenar_linhas(conn, &c.nome, &grupos, agora)?;
        armazenar_requests(conn, &c.nome, &entradas, agora)?;
        conn.execute(
            "INSERT OR REPLACE INTO containers (name, status, last_collected_at, uptime, criado_em)
             VALUES (?1, 'running', ?2, ?3, ?4)",
            rusqlite::params![c.nome, agora, c.status, c.criado_em],
        )?;
    }

    // 5. Retenção: descarta o que passou da validade.
    let corte = agora - (retencao_horas as i64) * 3600;
    prune_antigos(conn, corte)?;

    Ok(())
}

/// Sobe a thread coletora. Devolve o handle (para `join` na saída) e o
/// sender de comandos (para `ColetarAgora`/`Encerrar`).
pub fn iniciar_coletor(
    parametros: ParametrosColetor,
    eventos: mpsc::Sender<EventoColeta>,
) -> (thread::JoinHandle<()>, mpsc::Sender<ComandoColetor>) {
    let (tx_comandos, rx_comandos) = mpsc::channel::<ComandoColetor>();

    // `move`: a closure toma POSSE de `parametros`, `eventos` e
    // `rx_comandos` — obrigatório em `thread::spawn`, porque a thread pode
    // viver mais que a função que a criou (ownership transferido, não
    // emprestado).
    // docs: https://doc.rust-lang.org/book/ch16-01-threads.html#using-move-closures-with-threads
    let handle = thread::spawn(move || {
        // A conexão é criada DENTRO da thread (Connection não atravessa
        // threads com segurança). Falha ao abrir = avisa e morre; a TUI
        // mostra a falha.
        let conn = match Connection::open(&parametros.db) {
            Ok(c) => c,
            Err(erro) => {
                let _ = eventos.send(EventoColeta::Falha(format!(
                    "não abriu o banco {}: {erro}",
                    parametros.db.display()
                )));
                return;
            }
        };
        // WAL: escritor (esta thread) e leitores (TUI) convivem sem lock.
        // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.pragma_update
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        if let Err(erro) = init_db(&conn) {
            let _ = eventos.send(EventoColeta::Falha(format!("init_db falhou: {erro}")));
            return;
        }

        loop {
            // Roda um ciclo e anuncia o resultado. `let _ =` no send: se o
            // receptor já morreu (TUI fechou), não há o que fazer — o loop
            // termina no `recv_timeout` abaixo (Disconnected).
            match coletar_ciclo(
                &parametros.executor,
                &conn,
                parametros.tail_inicial,
                parametros.retencao_horas,
            ) {
                Ok(()) => {
                    let _ = eventos.send(EventoColeta::Novo);
                }
                Err(erro) => {
                    let _ = eventos.send(EventoColeta::Falha(erro.to_string()));
                }
            }

            // Espera o intervalo OU um comando — o `recv_timeout` faz os
            // dois papéis de uma vez (sleep interrompível).
            // docs: https://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html#method.recv_timeout
            match rx_comandos.recv_timeout(parametros.intervalo) {
                Ok(ComandoColetor::ColetarAgora) => continue,
                Ok(ComandoColetor::Encerrar) => break,
                // Canal fechado = o outro lado (TUI) sumiu; encerra junto.
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                // Timeout = passou o intervalo; próximo ciclo.
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
            }
        }
    });

    (handle, tx_comandos)
}
