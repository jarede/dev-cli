// CASCA DE IO: subcomando `logs remote`.
// Executa `docker logs` via SSH em hosts remotos, com coleta incremental
// opcional via SQLite, modo --watch e TUI interativa.

// `Write` traz o método `.flush()`, usado para forçar a stdout bufferizada a
// aparecer imediatamente no terminal (ver comentários mais abaixo).
// docs: https://doc.rust-lang.org/std/io/trait.Write.html#method.flush
use std::io::Write;
use std::path::PathBuf;
// `Duration` representa um intervalo de tempo (usado no `sleep` do modo
// `--watch`); `SystemTime`/`UNIX_EPOCH` servem para calcular o timestamp Unix
// (segundos desde 1970) que guardamos no banco como `collected_at`.
// docs: https://doc.rust-lang.org/std/time/struct.Duration.html
// docs: https://doc.rust-lang.org/std/time/struct.SystemTime.html
// docs: https://doc.rust-lang.org/std/time/constant.UNIX_EPOCH.html
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use clap::Args;
// Trait de extensão do `owo-colors`: ao importá-la, todo tipo que implementa
// `Display` ganha métodos como `.red()`, `.bold()`, `.dimmed()`.
// docs: https://docs.rs/owo-colors/latest/owo_colors/trait.OwoColorize.html
use owo_colors::OwoColorize;
use rusqlite::Connection;

use nucleo::core::{categorizar_por_nivel, contar_niveis_docker};
use nucleo::db::{armazenar_contagens, armazenar_linhas, init_db, verificar_status_containers};
use nucleo::executor::{ContainerDocker, Executor, listar_containers, obter_logs, obter_logs_desde};
use crate::logs::render::exibir_estatisticas;
use crate::logs::render::renderizar_container;

/// Estatísticas de logs de containers via SSH (executa `docker logs` no host remoto).
#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct RemoteArgs {
    /// Container específico; se omitido, varre todos os containers rodando.
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    container: Option<String>,
    /// Host SSH ("user@host") para coletar de uma VM remota.
    /// Sem esta flag, executa `docker` localmente (modo padrão na VM).
    #[arg(long)]
    ssh: Option<String>,
    /// Quantidade de linhas do final de cada container (últimas N linhas).
    /// Ignorado quando `--db` está ativo usa incremental `--since`.
    // `default_value_t = 1000`: variante de `default_value` para tipos que já
    // implementam `Default`/`FromStr` numéricos; evita ter que escrever "1000"
    // como string e deixar o clap fazer o parse.
    // docs: https://docs.rs/clap/latest/clap/_derive/index.html
    // docs: https://doc.rust-lang.org/std/default/trait.Default.html
    // docs: https://doc.rust-lang.org/std/str/trait.FromStr.html
    #[arg(long, default_value_t = 1000)]
    tail: usize,
    /// Caminho do banco SQLite para armazenamento incremental.
    // Sem `default_value`, um `#[arg(long)]` sobre `Option<T>` fica `None`
    // quando a flag não é passada — é assim que sabemos, mais abaixo, se o
    // usuário pediu persistência ou não.
    // docs: https://doc.rust-lang.org/std/option/enum.Option.html
    #[arg(long)]
    db: Option<PathBuf>,
    /// Modo contínuo: coleta a cada 5 minutos (requer --db).
    #[arg(short, long)]
    watch: bool,
    /// Abre TUI interativo para navegar nas estatísticas (requer --db).
    #[arg(long)]
    tui: bool,
}

impl RemoteArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Decide a estratégia: SSH só quando pedido; o padrão é docker local
        // (os binários rodam na própria VM que tem o docker).
        let executor = match &self.ssh {
            Some(host) => Executor::Ssh(host.clone()),
            None => Executor::Local,
        };

        // `let ... else`: tenta desestruturar o `Option`; se for `None`, o
        // bloco `else` OBRIGATORIAMENTE desvia o fluxo (aqui, com `return`)
        // antes de chegar na linha seguinte — diferente de `if let`, não há
        // como "continuar" sem um `db_path` válido depois deste ponto.
        // docs: https://doc.rust-lang.org/std/option/enum.Option.html
        let Some(db_path) = &self.db else {
            // Modo original: one-shot sem persistência
            let containers = if let Some(container) = &self.container {
                vec![ContainerDocker {
                    nome: container.clone(),
                    status: String::new(),
                    criado_em: String::new(),
                }]
            } else {
                listar_containers(&executor)?
            };
            let mut saida = String::new();
            for c in containers {
                let conteudo = obter_logs(&executor, &c.nome, self.tail)?;
                let niveis = contar_niveis_docker(&conteudo);
                let status = if c.status.is_empty() {
                    None
                } else {
                    Some(c.status.as_str())
                };
                saida.push_str(&renderizar_container(&c.nome, status, &niveis));
            }
            return Ok(saida.trim_end().to_string());
        };

        // Modo com banco: coleta incremental + persistência
        // `Connection::open` cria o arquivo SQLite se ele não existir ainda.
        // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.open
        let conn = Connection::open(db_path)?;
        init_db(&conn)?;

        // Se o DB está vazio ou o usuário quer TUI, fazemos uma coleta agora
        // `query_row` executa um SELECT que devolve no máximo 1 linha; o
        // closure `|r| r.get::<_, i64>(0)` extrai a coluna 0 como `i64`
        // (a "turbofish" `::<_, i64>` diz ao rusqlite o tipo esperado).
        // `unwrap_or(0)`: se a consulta falhar (ex.: tabela ainda sem uso),
        // tratamos como "0 linhas" em vez de propagar erro aqui.
        // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.query_row
        // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Row.html#method.get
        // docs: https://doc.rust-lang.org/std/result/enum.Result.html#method.unwrap_or
        let db_vazio = conn
            .query_row("SELECT COUNT(*) FROM containers", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap_or(0)
            == 0;

        if db_vazio || !self.tui {
            // `duration_since(UNIX_EPOCH)` dá o tempo decorrido desde a
            // "época Unix" (1970-01-01); `.as_secs()` extrai os segundos.
            // `unwrap_or_default()` cai para `Duration::ZERO` no caso (bem
            // improvável) do relógio do sistema estar antes de 1970.
            // docs: https://doc.rust-lang.org/std/time/struct.SystemTime.html#method.duration_since
            // docs: https://doc.rust-lang.org/std/time/struct.Duration.html#method.as_secs
            // docs: https://doc.rust-lang.org/std/result/enum.Result.html#method.unwrap_or_default
            // docs: https://doc.rust-lang.org/std/time/struct.Duration.html#associatedconstant.ZERO
            let agora = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let mut saida = String::new();

            // 1. Descobre containers rodando (ou usa o específico) e detecta paradas/restart
            let rodando = if let Some(nome) = &self.container {
                vec![ContainerDocker {
                    nome: nome.clone(),
                    status: String::new(),
                    criado_em: String::new(),
                }]
            } else {
                listar_containers(&executor)?
            };
            // `iter()` empresta cada `ContainerDocker` (sem consumir `rodando`,
            // que ainda usamos logo abaixo); `.clone()` copia só a `String`
            // do nome para o novo `Vec`.
            // docs: https://doc.rust-lang.org/std/vec/struct.Vec.html#method.iter
            // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.map
            // docs: https://doc.rust-lang.org/std/clone/trait.Clone.html#tymethod.clone
            // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.collect
            let nomes_rodando: Vec<String> = rodando.iter().map(|c| c.nome.clone()).collect();
            let alertas = verificar_status_containers(&conn, &nomes_rodando, agora)?;
            for alerta in &alertas {
                saida.push_str(&format!("⚠️  {}\n", alerta.bold()));
            }

            // 2. Coleta incremental dos que estão rodando
            for c in &rodando {
                // `rusqlite::params![...]` monta os valores para os `?1`, `?2`
                // etc. da query, escapando-os corretamente (evita SQL
                // injection). `COALESCE(..., 0)` troca `NULL` por `0` direto
                // no SQL, então o `.get(0)` sempre recebe um inteiro.
                // docs: https://docs.rs/rusqlite/latest/rusqlite/macro.params.html
                // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Row.html#method.get
                let ultima_coleta: i64 = conn
                    .query_row(
                        "SELECT COALESCE(last_collected_at, 0) FROM containers WHERE name = ?1",
                        rusqlite::params![c.nome],
                        |row| row.get(0),
                    )
                    .unwrap_or(0);

                let conteudo = if ultima_coleta == 0 {
                    obter_logs(&executor, &c.nome, self.tail)?
                } else {
                    obter_logs_desde(&executor, &c.nome, ultima_coleta)?
                };

                // Extrai linhas categorizadas por nível e persiste no banco
                let grupos = categorizar_por_nivel(&conteudo);
                // Reduz cada grupo (nível -> Vec<linha>) à sua contagem
                // (nível -> quantidade); `v.len()` é O(1) num `Vec`.
                // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.iter
                // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.map
                // docs: https://doc.rust-lang.org/std/vec/struct.Vec.html#method.len
                // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.collect
                let niveis: std::collections::BTreeMap<String, usize> =
                    grupos.iter().map(|(k, v)| (k.clone(), v.len())).collect();

                armazenar_contagens(&conn, &c.nome, &niveis, agora)?;
                armazenar_linhas(&conn, &c.nome, &grupos, agora)?;
                // `INSERT OR REPLACE`: se já existe uma linha com essa chave
                // primária (`name`), o SQLite substitui a linha inteira em vez
                // de falhar por violar unicidade — é o "upsert" mais simples
                // do SQLite.
                conn.execute(
                    "INSERT OR REPLACE INTO containers (name, status, last_collected_at, uptime, criado_em) VALUES (?1, 'running', ?2, ?3, ?4)",
                    rusqlite::params![c.nome, agora, c.status, c.criado_em],
                )?;
            }

            // 3. Exibe acumulado do banco (só se não for TUI)
            if !self.tui {
                saida.push_str(&exibir_estatisticas(&conn)?);

                if !self.watch {
                    return Ok(saida.trim_end().to_string());
                }

                // Modo --watch: limpa, exibe painel e espera 5 min
                // "\x1b[2J" (limpa a tela) + "\x1b[H" (move o cursor para o
                // canto superior esquerdo) são sequências de escape ANSI —
                // truque comum para simular um "dashboard" que se redesenha.
                print!("\x1b[2J\x1b[H{}", saida.trim_end());
                // `print!` só escreve no buffer interno da stdout; sem
                // `flush()` o terminal pode não mostrar nada até o processo
                // encerrar ou o buffer encher. O `?` propaga qualquer erro de
                // IO ao tentar escrever (raro, mas possível).
                // docs: https://doc.rust-lang.org/std/macro.print.html
                // docs: https://doc.rust-lang.org/std/io/trait.Write.html#tymethod.flush
                // docs: https://doc.rust-lang.org/std/io/fn.stdout.html
                std::io::stdout().flush()?;
                // Bloqueia esta thread (a única do programa aqui) por 5
                // minutos antes de seguir para a próxima instrução.
                // docs: https://doc.rust-lang.org/std/thread/fn.sleep.html
                // docs: https://doc.rust-lang.org/std/time/struct.Duration.html#method.from_secs
                std::thread::sleep(Duration::from_secs(300));
            }
        }

        // Modo TUI: entrega o controle do terminal para a interface
        // interativa (só retorna quando o usuário sai dela).
        crate::tui::run_tui(&conn)?;
        Ok(String::new())
    }
}
