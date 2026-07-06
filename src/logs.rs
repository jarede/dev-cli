// Módulo do subcomando `logs stats`.
//
// A ideia central de arquitetura aqui é separar duas responsabilidades:
//   1. NÚCLEO PURO  -> `fn contar`: recebe texto, devolve contagens. Não toca
//      em disco nem imprime nada. Por ser puro, é 100% testável com strings
//      inline (ver o módulo `tests` no final).
//   2. CASCA DE IO  -> `StatsArgs::execute` e `descobrir_alvos`: descobrem os
//      arquivos, leem do disco e formatam a saída colorida.
// Manter o cálculo separado do efeito é um padrão que facilita testar e raciocinar.

use std::collections::BTreeMap;
// `BufRead` traz o método `.lines()` para leitores bufferizados (usado no
// modo `-f`, lendo o stdout do processo filho linha a linha).
// docs: https://doc.rust-lang.org/std/io/trait.BufRead.html#method.lines
use std::io::BufRead;
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

// `Args`/`Subcommand`: macros de derive do clap que geram, a partir dos
// campos/variantes anotados, o parser de linha de comando (flags, posicionais,
// valores default, help text) sem precisarmos escrever isso à mão.
// docs: https://docs.rs/clap/latest/clap/trait.Args.html
// docs: https://docs.rs/clap/latest/clap/trait.Subcommand.html
use clap::Args;
use clap::Subcommand;
// Trait de extensão do `owo-colors`: ao importá-la, todo tipo que implementa
// `Display` ganha métodos como `.red()`, `.bold()`, `.dimmed()`.
// docs: https://docs.rs/owo-colors/latest/owo_colors/trait.OwoColorize.html
use owo_colors::OwoColorize;
use rusqlite::Connection;

/// Metadados de um container obtidos via `docker ps`.
struct ContainerRemoto {
    nome: String,
    /// Status textual: "Up 2 days", "Exited (0) 3 days ago", etc.
    status: String,
    /// Timestamp ISO de criação: "2026-07-04 12:00:00 +0000 UTC"
    criado_em: String,
}

// Níveis que o supervisord escreve na 3ª coluna de cada linha própria.
// `[&str; 6]` é um array de tamanho fixo conhecido em tempo de compilação.
const NIVEIS_CONHECIDOS: [&str; 6] = ["INFO", "DEBG", "WARN", "CRIT", "ERRO", "TRAC"];
// Palavras que procuramos no TEXTO da linha (logs da app embutidos no stdout).
const PALAVRAS_CHAVE: [&str; 4] = ["error", "warn", "info", "debug"];

/// Comandos de log.
// `#[derive(Args, Debug)]`: `Args` faz o clap tratar esta struct como um
// grupo de argumentos (aqui, apenas o subcomando aninhado); `Debug` gera
// automaticamente a impressão `{:?}`, útil para inspecionar em depuração.
// `#[command(help_template = ...)]` troca o texto de ajuda padrão do clap
// pelo template compartilhado definido em `crate::help`.
// docs: https://docs.rs/clap/latest/clap/trait.Args.html
// docs: https://doc.rust-lang.org/std/fmt/trait.Debug.html
// docs: https://docs.rs/clap/latest/clap/_derive/index.html
#[derive(Args, Debug)]
#[command(help_template = crate::help::SUBCOMANDOS)]
pub struct LogsArgs {
    // `logs` é um grupo: ele apenas encaminha para um subcomando aninhado.
    #[command(subcommand)]
    comando: LogsCommands,
}

impl LogsArgs {
    // `&self` = empréstimo imutável; só lemos os campos, não os consumimos.
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // `match` sobre a referência do enum; o `match` exaustivo obriga a
        // tratar cada variante nova assim que ela for adicionada.
        match &self.comando {
            LogsCommands::Stats(args) => args.execute(),
            LogsCommands::Containers(args) => args.execute(),
            LogsCommands::Remote(args) => args.execute(),
        }
    }
}

/// Subcomandos de `logs`.
// `#[derive(Subcommand, Debug)]`: `Subcommand` faz o clap gerar o parser que
// decide qual variante (e portanto qual `*Args`) instanciar a partir da
// palavra digitada pelo usuário (`stats`, `containers` ou `remote`).
// docs: https://docs.rs/clap/latest/clap/trait.Subcommand.html
// docs: https://doc.rust-lang.org/std/fmt/trait.Debug.html
#[derive(Subcommand, Debug)]
enum LogsCommands {
    /// Estatísticas de logs de containers (arquivos supervisord).
    Stats(StatsArgs),
    /// Estatísticas de logs dos containers detectados via `container list`.
    Containers(ContainersArgs),
    /// Estatísticas de logs de containers via SSH (docker logs remoto).
    Remote(RemoteArgs),
}

/// Estatísticas de logs de um container específico, ou de todos.
#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct StatsArgs {
    /// Container específico; se omitido, varre todos em --path.
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    container: Option<String>,
    /// Caminho do diretório com os logs dos containers.
    // `#[arg(long, default_value = "dados/logs", ...)]`: `long` expõe o campo
    // como `--path <valor>`; `default_value` faz o clap preencher `path`
    // automaticamente quando a flag não é passada, então o campo não precisa
    // ser `Option<PathBuf>`.
    // docs: https://docs.rs/clap/latest/clap/_derive/index.html
    #[arg(long, default_value = "dados/logs", help_heading = crate::help::OPCOES)]
    path: PathBuf,
}

impl StatsArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // Descobre quais arquivos ler. O `?` propaga o erro para cima se falhar.
        let alvos = self.descobrir_alvos()?;
        // Acumulador da saída. `mut` porque vamos anexar texto a cada iteração.
        let mut saida = String::new();
        // `alvos` é um Vec de tuplas (nome, caminho); o `for` consome cada uma.
        for (nome, caminho) in alvos {
            // Lê o arquivo inteiro para uma String. `map_err` troca o erro cru
            // de IO por uma mensagem clara com o caminho antes do `?` propagar.
            // docs: https://doc.rust-lang.org/std/result/enum.Result.html#method.map_err
            // docs: https://doc.rust-lang.org/std/fs/fn.read_to_string.html
            let conteudo = std::fs::read_to_string(&caminho)
                .map_err(|erro| format!("não foi possível ler '{}': {erro}", caminho.display()))?;
            // NÚCLEO PURO: transforma texto em contagens.
            let contagens = contar(&conteudo);
            // Formata o bloco colorido deste container e anexa ao acumulador.
            saida.push_str(&renderizar(&nome, &contagens));
        }
        // `trim_end` remove a quebra de linha final; devolvemos a String pronta
        // (quem imprime é o `main.rs`).
        Ok(saida.trim_end().to_string())
    }

    // Resolve a lista de arquivos a processar: um único container ou todos.
    fn descobrir_alvos(&self) -> Result<Vec<(String, PathBuf)>, Box<dyn std::error::Error>> {
        // `if let Some(...)` desestrutura o Option: só entra se o usuário passou container.
        // docs: https://doc.rust-lang.org/std/option/enum.Option.html
        if let Some(container) = &self.container {
            // `join` monta o caminho `<path>/<container>/supervisord.log`.
            // docs: https://doc.rust-lang.org/std/path/struct.Path.html#method.join
            let caminho = self.path.join(container).join("supervisord.log");
            if !caminho.exists() {
                // `.into()` converte a String para o `Box<dyn Error>` do retorno.
                // docs: https://doc.rust-lang.org/std/convert/trait.Into.html
                return Err(format!(
                    "arquivo de log não encontrado para o container '{container}': '{}'",
                    caminho.display()
                )
                .into());
            }
            // `vec![...]` cria um Vec com um único elemento.
            // docs: https://doc.rust-lang.org/std/macro.vec.html
            return Ok(vec![(container.clone(), caminho)]);
        }

        // Sem container: listamos as entradas do diretório `--path`.
        // docs: https://doc.rust-lang.org/std/fs/fn.read_dir.html
        let entradas = std::fs::read_dir(&self.path).map_err(|erro| {
            format!(
                "não foi possível ler o diretório '{}': {erro}",
                self.path.display()
            )
        })?;

        let mut nomes = Vec::new();
        // `read_dir` produz um iterador de `Result<DirEntry>`: cada leitura pode falhar.
        // docs: https://doc.rust-lang.org/std/fs/struct.DirEntry.html
        for entrada in entradas {
            let entrada = entrada?;
            // Só nos interessam subdiretórios (um por container). A "let chain"
            // com `&& let` encadeia duas condições num único `if`: só empurra o
            // nome se for diretório E o nome for UTF-8 válido (`to_str()` devolve
            // `Option<&str>`, `None` se não for).
            // docs: https://doc.rust-lang.org/std/fs/struct.DirEntry.html#method.file_type
            // docs: https://doc.rust-lang.org/std/fs/struct.FileType.html#method.is_dir
            // docs: https://doc.rust-lang.org/std/fs/struct.DirEntry.html#method.file_name
            // docs: https://doc.rust-lang.org/std/ffi/struct.OsStr.html#method.to_str
            if entrada.file_type()?.is_dir()
                && let Some(nome) = entrada.file_name().to_str()
            {
                nomes.push(nome.to_string());
            }
        }
        // A ordem do `read_dir` não é garantida; ordenamos para saída estável.
        // docs: https://doc.rust-lang.org/std/vec/struct.Vec.html#method.sort
        nomes.sort();

        // Transforma cada nome em uma tupla (nome, caminho do log).
        // `into_iter` consome o Vec; `map` adapta cada item; `collect` remonta um Vec.
        // docs: https://doc.rust-lang.org/std/iter/trait.IntoIterator.html#tymethod.into_iter
        // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.map
        // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.collect
        Ok(nomes
            .into_iter()
            .map(|nome| {
                let caminho = self.path.join(&nome).join("supervisord.log");
                (nome, caminho)
            })
            .collect())
    }
}

// Níveis que a saída de `container logs` costuma usar (formatos ao estilo
// das crates `tracing`/`log`, ou de logging do Python). Comparamos sempre em
// maiúsculo, então "warn" e "WARN" caem na mesma chave.
const NIVEIS_CONTAINER: [&str; 8] = [
    "TRACE", "DEBUG", "INFO", "WARN", "WARNING", "ERROR", "CRITICAL", "FATAL",
];

/// Estatísticas de logs de todos os containers detectados via `container list`.
#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct ContainersArgs {
    /// Container específico; se omitido, varre todos os detectados por `container list`.
    // `Option<String>`: o clap deixa esse argumento posicional opcional porque
    // o tipo é `Option`; sem valor na linha de comando, vira `None`.
    // docs: https://doc.rust-lang.org/std/option/enum.Option.html
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    container: Option<String>,
    /// Acompanha os logs em tempo real (como `tail -f`), redesenhando o painel a cada linha nova.
    // `bool` com `#[arg(short, long)]`: o clap trata automaticamente como uma
    // flag (`-f`/`--follow`) que não recebe valor — presente = `true`.
    // docs: https://docs.rs/clap/latest/clap/_derive/index.html
    #[arg(short = 'f', long, help_heading = crate::help::OPCOES)]
    follow: bool,
}

impl ContainersArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // `if let ... else`: um único container pedido vira um Vec de 1;
        // sem container, perguntamos ao binário `container` quem está rodando.
        let nomes = if let Some(container) = &self.container {
            vec![container.clone()]
        } else {
            listar_containers()?
        };

        if self.follow {
            // Modo ao vivo: fica bloqueado imprimindo atualizações direto no
            // terminal. Só retorna quando todos os containers pararem de logar
            // (ou nunca, se o usuário encerrar com Ctrl+C antes disso).
            seguir_containers(&nomes)?;
            return Ok(String::new());
        }

        let mut saida = String::new();
        for nome in nomes {
            let conteudo = obter_logs(&nome)?;
            // NÚCLEO PURO: mesmo princípio do `contar` acima, mas adaptado ao
            // formato colorido (códigos ANSI) que `container logs` emite.
            let niveis = contar_niveis_container(&conteudo);
            saida.push_str(&renderizar_container(&nome, None, &niveis));
        }

        Ok(saida.trim_end().to_string())
    }
}

/// Estatísticas de logs de containers via SSH (executa `docker logs` no host remoto).
#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct RemoteArgs {
    /// Container específico; se omitido, varre todos os containers rodando.
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    container: Option<String>,
    /// Host SSH (user@host).
    #[arg(long, default_value = "jarede.silva@qa.bistek.com.br")]
    host: String,
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
        // `let ... else`: tenta desestruturar o `Option`; se for `None`, o
        // bloco `else` OBRIGATORIAMENTE desvia o fluxo (aqui, com `return`)
        // antes de chegar na linha seguinte — diferente de `if let`, não há
        // como "continuar" sem um `db_path` válido depois deste ponto.
        // docs: https://doc.rust-lang.org/std/option/enum.Option.html
        let Some(db_path) = &self.db else {
            // Modo original: one-shot sem persistência
            let containers = if let Some(container) = &self.container {
                vec![ContainerRemoto {
                    nome: container.clone(),
                    status: String::new(),
                    criado_em: String::new(),
                }]
            } else {
                listar_containers_remoto(&self.host)?
            };
            let mut saida = String::new();
            for c in containers {
                let conteudo = obter_logs_remoto(&self.host, &c.nome, self.tail)?;
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

            // 1. Descobre containers rodando e detecta paradas/restart
            let rodando = listar_containers_remoto(&self.host)?;
            // `iter()` empresta cada `ContainerRemoto` (sem consumir `rodando`,
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
                    obter_logs_remoto(&self.host, &c.nome, self.tail)?
                } else {
                    obter_logs_remoto_desde(&self.host, &c.nome, ultima_coleta)?
                };

                // Extrai linhas categorizadas por nível e persiste no banco
                let grupos = categorizar_por_nivel(&conteudo);
                // Reduz cada grupo (nível -> Vec<linha>) à sua contagem
                // (nível -> quantidade); `v.len()` é O(1) num `Vec`.
                // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.iter
                // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.map
                // docs: https://doc.rust-lang.org/std/vec/struct.Vec.html#method.len
                // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.collect
                let niveis: BTreeMap<String, usize> =
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

// Cria as tabelas do banco se não existirem.
// docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.execute_batch
fn init_db(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS containers (
            name TEXT PRIMARY KEY,
            status TEXT NOT NULL DEFAULT 'unknown',
            last_collected_at INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS log_counts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            container_name TEXT NOT NULL,
            level TEXT NOT NULL,
            count INTEGER NOT NULL DEFAULT 0,
            collected_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS alerts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            container_name TEXT NOT NULL,
            alert_type TEXT NOT NULL,
            message TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS log_lines (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            container_name TEXT NOT NULL,
            level TEXT NOT NULL,
            line TEXT NOT NULL,
            collected_at INTEGER NOT NULL
        );",
    )?;

    // Migração: adiciona colunas que podem não existir em DBs criados antes
    // (o `CREATE TABLE IF NOT EXISTS` acima não altera tabelas já existentes).
    // `&[...]` é um array de literais `&str` percorrido por referência.
    for sql in &[
        "ALTER TABLE containers ADD COLUMN uptime TEXT DEFAULT ''",
        "ALTER TABLE containers ADD COLUMN criado_em TEXT DEFAULT ''",
    ] {
        // `let _ = ...` descarta o `Result` de propósito: se a coluna já
        // existir, o SQLite retorna erro e é exatamente isso que ignoramos
        // aqui (idempotência da migração), sem propagar para o `?` do retorno.
        // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.execute
        let _ = conn.execute(sql, []);
    }

    Ok(())
}

// Insere as contagens desta coleta no banco.
fn armazenar_contagens(
    conn: &Connection,
    nome: &str,
    niveis: &BTreeMap<String, usize>,
    agora: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    // `prepare` compila o SQL uma única vez; `stmt.execute` é chamado depois
    // dentro do loop, reaproveitando a mesma statement preparada (mais
    // eficiente do que preparar um SQL novo a cada nível).
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Connection.html#method.prepare
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Statement.html#method.execute
    let mut stmt =
        conn.prepare("INSERT INTO log_counts (container_name, level, count, collected_at) VALUES (?1, ?2, ?3, ?4)")?;
    // `for (nivel, &quantidade) in niveis`: itera pelas entradas do
    // `BTreeMap` desestruturando a tupla `(&String, &usize)`; o padrão
    // `&quantidade` copia o `usize` para fora da referência (tipo `Copy`).
    // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
    // docs: https://doc.rust-lang.org/std/marker/trait.Copy.html
    for (nivel, &quantidade) in niveis {
        if quantidade > 0 {
            // docs: https://docs.rs/rusqlite/latest/rusqlite/macro.params.html
            stmt.execute(rusqlite::params![nome, nivel, quantidade as i64, agora])?;
        }
    }
    Ok(())
}

// NÚCLEO PURO: categoriza cada linha de log pelo nível detectado.
// Devolve um mapa de nível → lista de linhas (já sem códigos ANSI).
fn categorizar_por_nivel(conteudo: &str) -> BTreeMap<String, Vec<String>> {
    let mut grupos: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for linha in conteudo.lines() {
        let limpa = remover_ansi(linha);
        if let Some(nivel) = limpa
            .split_whitespace()
            .find(|token| NIVEIS_DOCKER.contains(&token.to_uppercase().as_str()))
        {
            // `entry(...).or_default()`: se o nível ainda não é chave do mapa,
            // insere um `Vec` vazio (o `Default` de `Vec<String>`); em
            // seguida `.push(limpa)` empilha a linha nesse `Vec`, seja ele
            // recém-criado ou já existente.
            // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.entry
            // docs: https://doc.rust-lang.org/std/collections/btree_map/enum.Entry.html#method.or_default
            // docs: https://doc.rust-lang.org/std/vec/struct.Vec.html#method.push
            grupos.entry(nivel.to_uppercase()).or_default().push(limpa);
        }
    }
    grupos
}

// CASCA DE IO: armazena as linhas de log no banco, agrupadas por nível.
// Remove linhas antigas do mesmo container para evitar acúmulo infinito.
fn armazenar_linhas(
    conn: &Connection,
    nome: &str,
    grupos: &BTreeMap<String, Vec<String>>,
    agora: i64,
) -> Result<(), Box<dyn std::error::Error>> {
    // Remove linhas antigas deste container (mantém só as últimas coletas)
    conn.execute(
        "DELETE FROM log_lines WHERE container_name = ?1",
        rusqlite::params![nome],
    )?;

    let mut stmt = conn.prepare(
        "INSERT INTO log_lines (container_name, level, line, collected_at) VALUES (?1, ?2, ?3, ?4)",
    )?;
    for (nivel, linhas) in grupos {
        for linha in linhas {
            stmt.execute(rusqlite::params![nome, nivel, linha, agora])?;
        }
    }
    Ok(())
}

// Compara containers conhecidos no DB com os que estão rodando agora.
// Gera alertas para containers que pararam ou reiniciaram.
fn verificar_status_containers(
    conn: &Connection,
    rodando: &[String],
    agora: i64,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut alertas = Vec::new();

    // Containers que estavam running mas não estão mais → pararam
    let mut stmt = conn.prepare("SELECT name FROM containers WHERE status = 'running'")?;
    // `query_map` devolve um iterador de `Result<String, rusqlite::Error>`
    // (uma linha pode falhar ao ser convertida). `filter_map(|r| r.ok())`
    // descarta silenciosamente qualquer linha com erro e mantém só os `Ok`,
    // convertendo cada `Result` em `Option` e já "achatando" o iterador.
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Statement.html#method.query_map
    // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.filter_map
    // docs: https://doc.rust-lang.org/std/result/enum.Result.html#method.ok
    // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.collect
    let conhecidos: Vec<String> = stmt
        .query_map([], |row| row.get(0))?
        .filter_map(|r| r.ok())
        .collect();

    for nome in &conhecidos {
        if !rodando.contains(nome) {
            conn.execute(
                "UPDATE containers SET status = 'stopped' WHERE name = ?1",
                rusqlite::params![nome],
            )?;
            conn.execute(
                "INSERT INTO alerts (container_name, alert_type, message, created_at) VALUES (?1, 'stopped', ?2, ?3)",
                rusqlite::params![nome, format!("Container '{nome}' parou"), agora],
            )?;
            alertas.push(format!("⚠️  {} PAROU", nome));
        }
    }

    // Containers rodando agora mas estavam stopped → reiniciaram
    for nome in rodando {
        // `.ok()` converte o `Result<String, _>` em `Option<String>`,
        // tratando "não achei essa linha" e "erro de SQL" da mesma forma:
        // simplesmente `None` (sem status anterior conhecido).
        // docs: https://doc.rust-lang.org/std/result/enum.Result.html#method.ok
        let status_anterior: Option<String> = conn
            .query_row(
                "SELECT status FROM containers WHERE name = ?1",
                rusqlite::params![nome],
                |row| row.get(0),
            )
            .ok();

        // Let chain (edition 2024): só entra no bloco se `status_anterior`
        // for `Some` E o valor dentro for exatamente "stopped" — equivalente
        // a um `if aninhado`, mas sem o aninhamento (evita o lint
        // `collapsible_if`). `.as_ref()` empresta o `String` de dentro do
        // `Option` em vez de movê-lo, porque ainda usamos `status_anterior`
        // implicitamente via `status` logo abaixo.
        // docs: https://doc.rust-lang.org/std/option/enum.Option.html#method.as_ref
        if let Some(status) = status_anterior.as_ref()
            && status == "stopped"
        {
            conn.execute(
                "INSERT INTO alerts (container_name, alert_type, message, created_at) VALUES (?1, 'restarted', ?2, ?3)",
                rusqlite::params![nome, format!("Container '{nome}' reiniciou"), agora],
            )?;
            alertas.push(format!("🔄 {} REINICIOU", nome));
        }
    }

    Ok(alertas)
}

// Lê as contagens acumuladas do banco e formata para exibição.
fn exibir_estatisticas(conn: &Connection) -> Result<String, Box<dyn std::error::Error>> {
    let mut stmt = conn.prepare(
        "SELECT container_name, level, SUM(count) as total
         FROM log_counts
         GROUP BY container_name, level
         ORDER BY container_name, level",
    )?;

    // Mapa aninhado: container -> (nível -> total). O `BTreeMap` externo dá
    // ordem alfabética por container; o interno, por nível.
    let mut dados: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    // O closure de `query_map` roda por linha e pode falhar em cada `row.get`
    // (tipo errado, coluna ausente); por isso ele próprio devolve `Result`,
    // e usamos `?` dentro dele para propagar esse erro por linha.
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Statement.html#method.query_map
    // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Row.html#method.get
    let linhas = stmt.query_map([], |row| {
        let nome: String = row.get(0)?;
        let nivel: String = row.get(1)?;
        let total: i64 = row.get(2)?;
        Ok((nome, nivel, total as usize))
    })?;

    for linha in linhas {
        // Aqui o `?` é sobre o `Result` de CADA linha (o iterador de
        // `query_map` produz `Result<(...), Error>`), não sobre o closure
        // acima.
        let (nome, nivel, total) = linha?;
        // API `entry`: garante um `BTreeMap` vazio para containers novos
        // antes de inserir o par nível/total.
        // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.entry
        // docs: https://doc.rust-lang.org/std/collections/btree_map/enum.Entry.html#method.or_default
        // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.insert
        dados.entry(nome).or_default().insert(nivel, total);
    }

    // Carrega o status (uptime) de cada container do banco
    let mut stmt2 = conn
        .prepare("SELECT name, uptime FROM containers WHERE uptime IS NOT NULL AND uptime != ''")?;
    let mut status_map: BTreeMap<String, String> = BTreeMap::new();
    // `.flatten()` sobre um iterador de `Result<(String, String), Error>`
    // funciona porque `Result` também implementa `IntoIterator` (0 ou 1
    // item): `Ok(x)` vira um iterador de 1 elemento, `Err(_)` vira vazio —
    // é uma forma mais curta de "ignore os erros e siga com os `Ok`".
    // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.flatten
    // docs: https://doc.rust-lang.org/std/result/enum.Result.html
    // docs: https://doc.rust-lang.org/std/iter/trait.IntoIterator.html
    for row in stmt2
        .query_map([], |r| {
            let n: String = r.get(0)?;
            let s: String = r.get(1)?;
            Ok((n, s))
        })?
        .flatten()
    {
        status_map.insert(row.0, row.1);
    }

    let mut saida = String::new();
    for (nome, niveis) in &dados {
        let status = status_map.get(nome).map(|s| s.as_str());
        saida.push_str(&renderizar_container(nome, status, niveis));
    }
    Ok(saida)
}

// CASCA DE IO: busca logs de um container desde um timestamp Unix (segundos).
fn obter_logs_remoto_desde(
    host: &str,
    nome: &str,
    desde: i64,
) -> Result<String, Box<dyn std::error::Error>> {
    let cmd = format!("docker logs --since {desde} {nome}");
    let saida = std::process::Command::new("ssh")
        .args([host, &cmd])
        .output()
        .map_err(|erro| format!("falha ao obter logs incrementais de '{nome}' via SSH: {erro}"))?;

    if !saida.status.success() {
        return Err(format!(
            "'docker logs --since {desde} {nome}' via SSH terminou com erro: {}",
            String::from_utf8_lossy(&saida.stderr)
        )
        .into());
    }

    Ok(String::from_utf8_lossy(&saida.stdout).to_string())
}

// CASCA DE IO: pergunta ao host remoto quais containers estão rodando com
// status e timestamp de criação (uptime).
fn listar_containers_remoto(
    host: &str,
) -> Result<Vec<ContainerRemoto>, Box<dyn std::error::Error>> {
    let saida = std::process::Command::new("ssh")
        .args([
            host,
            "docker ps --format '{{.Names}}|{{.Status}}|{{.CreatedAt}}'",
        ])
        .output()
        .map_err(|erro| format!("falha ao conectar via SSH em {host}: {erro}"))?;

    if !saida.status.success() {
        return Err(format!(
            "SSH para {host} terminou com erro: {}",
            String::from_utf8_lossy(&saida.stderr)
        )
        .into());
    }

    Ok(String::from_utf8_lossy(&saida.stdout)
        .lines()
        .map(str::trim)
        .filter(|linha| !linha.is_empty())
        .filter_map(|linha| {
            let mut partes = linha.splitn(3, '|');
            Some(ContainerRemoto {
                nome: partes.next()?.to_string(),
                status: partes.next()?.to_string(),
                criado_em: partes.next()?.to_string(),
            })
        })
        .collect())
}

// CASCA DE IO: busca as últimas N linhas do log de um container via SSH.
// Se `tail` for 0, obtém TODAS as linhas (sem `--tail`).
fn obter_logs_remoto(
    host: &str,
    nome: &str,
    tail: usize,
) -> Result<String, Box<dyn std::error::Error>> {
    let cmd = if tail > 0 {
        format!("docker logs --tail {tail} {nome}")
    } else {
        format!("docker logs {nome}")
    };
    let saida = std::process::Command::new("ssh")
        .args([host, &cmd])
        .output()
        .map_err(|erro| format!("falha ao obter logs de '{nome}' via SSH: {erro}"))?;

    if !saida.status.success() {
        return Err(format!(
            "'docker logs {nome}' via SSH terminou com erro: {}",
            String::from_utf8_lossy(&saida.stderr)
        )
        .into());
    }

    Ok(String::from_utf8_lossy(&saida.stdout).to_string())
}

// CASCA DE IO: modo `-f`. Abre um processo `container logs -f <nome>` por
// container; cada um é lido numa thread própria (senão o `for` bloqueante de
// uma leitura travaria a leitura das outras). As threads só enviam linhas por
// um canal `mpsc`; quem acumula contagens e redesenha o painel é a thread
// principal, evitando qualquer mutex compartilhado entre elas.
fn seguir_containers(nomes: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // `mpsc` = multi-producer, single-consumer: uma thread por container
    // (produtoras) alimentam um único receptor aqui na thread principal.
    // docs: https://doc.rust-lang.org/std/sync/mpsc/fn.channel.html
    let (tx, rx) = std::sync::mpsc::channel::<(String, String)>();

    for nome in nomes {
        let mut child = std::process::Command::new("container")
            .args(["logs", "-f", nome])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|erro| format!("falha ao executar 'container logs -f {nome}': {erro}"))?;

        // `take()` retira o stdout de dentro do `Child` (deixando `None` no
        // lugar) para podermos movê-lo para a thread de leitura junto com o
        // resto do `child` (que precisamos manter vivo até o processo acabar).
        // docs: https://doc.rust-lang.org/std/option/enum.Option.html#method.take
        // docs: https://doc.rust-lang.org/std/option/enum.Option.html#method.ok_or
        // docs: https://doc.rust-lang.org/std/process/struct.Child.html#structfield.stdout
        let stdout = child
            .stdout
            .take()
            .ok_or("não foi possível capturar a saída do container")?;

        let tx = tx.clone();
        let nome_da_thread = nome.clone();
        std::thread::spawn(move || {
            let leitor = std::io::BufReader::new(stdout);
            // `lines()` bloqueia entre chamadas até a próxima linha chegar —
            // é isso que dá o efeito de "tempo real"; `map_while(Result::ok)`
            // ignora erros de leitura e para no primeiro (ex.: pipe fechado).
            // docs: https://doc.rust-lang.org/std/io/trait.BufRead.html#method.lines
            // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.map_while
            for linha in leitor.lines().map_while(Result::ok) {
                // Erro de `send` só ocorre se a thread principal já saiu;
                // ignoramos porque não há mais ninguém para avisar.
                // docs: https://doc.rust-lang.org/std/sync/mpsc/struct.Sender.html#method.send
                let _ = tx.send((nome_da_thread.clone(), linha));
            }
            // Espera o processo `container logs -f` encerrar de fato (evita
            // deixá-lo como zumbi quando o container para).
            // docs: https://doc.rust-lang.org/std/process/struct.Child.html#method.wait
            let _ = child.wait();
        });
    }
    // Descarta nosso clone original do transmissor: o canal só fecha (e o
    // `for` abaixo termina) quando TODAS as threads soltarem o `tx` delas.
    // docs: https://doc.rust-lang.org/std/mem/fn.drop.html
    drop(tx);

    // Contagem acumulada por container, viva só na thread principal (não
    // precisa de `Mutex`: nenhuma outra thread toca nela, só recebem por canal).
    // `collect()` monta o `BTreeMap` a partir de um iterador de tuplas
    // `(chave, valor)` — cada container começa com um mapa de níveis vazio.
    // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.collect
    let mut totais: BTreeMap<String, BTreeMap<String, usize>> = nomes
        .iter()
        .map(|nome| (nome.clone(), BTreeMap::new()))
        .collect();

    // `Receiver` (o `rx`) implementa `Iterator`: o `for` bloqueia esperando a
    // próxima mensagem e só termina quando todos os `tx` (um por thread) forem
    // descartados, ou seja, quando todas as threads produtoras acabarem.
    // docs: https://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html
    for (nome, linha) in rx {
        let niveis_da_linha = contar_niveis_container(&linha);
        // `entry(nome).or_default()`: garante um `BTreeMap<String, usize>`
        // vazio para containers que ainda não apareceram no canal, antes de
        // somar os níveis desta linha nele.
        // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.entry
        // docs: https://doc.rust-lang.org/std/collections/btree_map/enum.Entry.html#method.or_default
        let acumulado = totais.entry(nome).or_default();
        for (nivel, quantidade) in niveis_da_linha {
            // Mesmo idiom de `entry(...).or_insert(0) +=` usado em `contar` e
            // `contar_niveis_container`: soma `quantidade` ao total daquele
            // nível, partindo de `0` se for a primeira ocorrência.
            // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.entry
            // docs: https://doc.rust-lang.org/std/collections/btree_map/enum.Entry.html#method.or_insert
            *acumulado.entry(nivel).or_insert(0) += quantidade;
        }

        // Redesenha o painel inteiro a cada linha nova: limpa a tela e move o
        // cursor para o topo (códigos ANSI), como um dashboard ao vivo.
        // docs: https://doc.rust-lang.org/std/macro.print.html
        print!("\x1b[2J\x1b[H");
        for nome in nomes {
            if let Some(niveis) = totais.get(nome) {
                print!("{}", renderizar_container(nome, None, niveis));
            }
        }
        // `print!` só escreve no buffer da stdout; sem `flush` o painel não
        // apareceria até o buffer encher.
        // docs: https://doc.rust-lang.org/std/io/trait.Write.html#tymethod.flush
        std::io::stdout().flush()?;
    }

    Ok(())
}

// CASCA DE IO: pergunta ao binário `container` quais containers existem.
// `-q` faz o comando devolver só os IDs/nomes, um por linha.
fn listar_containers() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // `Command::new(...).args([...]).output()` executa o processo, espera ele
    // terminar e captura stdout/stderr/status de uma vez (diferente de
    // `spawn()`, que devolve o `Child` já em execução para controlarmos nós
    // mesmos — usado em `seguir_containers` para o modo `-f`).
    let saida = std::process::Command::new("container")
        .args(["list", "-q"])
        .output()
        .map_err(|erro| format!("falha ao executar 'container list': {erro}"))?;

    if !saida.status.success() {
        return Err(format!(
            "'container list' terminou com erro: {}",
            String::from_utf8_lossy(&saida.stderr)
        )
        .into());
    }

    // `stdout`/`stderr` vêm como `Vec<u8>` (bytes crus); `from_utf8_lossy`
    // converte para `str` trocando bytes inválidos por `�` em vez de falhar.
    Ok(String::from_utf8_lossy(&saida.stdout)
        .lines()
        // `str::trim`/`str::to_string` usados como valor (sem `|x| x.trim()`):
        // toda função de um argumento pode virar closure implícita em `map`.
        .map(str::trim)
        .filter(|linha| !linha.is_empty())
        .map(str::to_string)
        .collect())
}

// CASCA DE IO: busca o log completo de um container via `container logs`.
fn obter_logs(nome: &str) -> Result<String, Box<dyn std::error::Error>> {
    let saida = std::process::Command::new("container")
        .args(["logs", nome])
        .output()
        .map_err(|erro| format!("falha ao executar 'container logs {nome}': {erro}"))?;

    if !saida.status.success() {
        return Err(format!(
            "'container logs {nome}' terminou com erro: {}",
            String::from_utf8_lossy(&saida.stderr)
        )
        .into());
    }

    Ok(String::from_utf8_lossy(&saida.stdout).to_string())
}

// Remove sequências de escape ANSI (ex.: "\x1b[32m") de uma linha, deixando
// só o texto visível. `container logs` colore a saída, o que atrapalharia a
// busca pelo token do nível se não fosse removido antes.
fn remover_ansi(linha: &str) -> String {
    // `with_capacity` pré-aloca o buffer no tamanho da linha original: como
    // só removemos caracteres, o resultado nunca é maior, evitando
    // realocações durante os `push`.
    let mut limpa = String::with_capacity(linha.len());
    // `chars()` é um iterador sobre os `char` (Unicode) da string; guardamos
    // ele numa variável `mut` porque o `for` interno (`chars.by_ref()`)
    // precisa avançá-lo manualmente, então não pode ser um `for` simples aqui.
    let mut chars = linha.chars();
    // `while let Some(c) = chars.next()`: repete enquanto o iterador ainda
    // devolver itens; equivale a um `for c in chars` de baixo nível, mas
    // permite avançar o iterador "extra" dentro do laço (no `by_ref()` abaixo).
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Descarta tudo até o 'm' que fecha o código de escape (formato
            // "\x1b[<códigos>m"); `by_ref()` evita mover o iterador inteiro.
            for ch in chars.by_ref() {
                if ch == 'm' {
                    break;
                }
            }
        } else {
            limpa.push(c);
        }
    }
    limpa
}

// NÚCLEO PURO: conta ocorrências de cada nível na 2ª coluna de cada linha
// (depois da data), já sem códigos ANSI. Também é chamada linha a linha pelo
// modo `-f` (uma linha por vez), então precisa funcionar tanto para um texto
// gigante quanto para uma única linha recém-chegada.
fn contar_niveis_container(conteudo: &str) -> BTreeMap<String, usize> {
    let mut niveis = BTreeMap::new();
    for linha in conteudo.lines() {
        let limpa = remover_ansi(linha);
        // Índice 1 (2º token) = nível: o formato de `container logs` é
        // "<data> <nível> <origem>: <mensagem>", sem hora junto da data.
        if let Some(token) = limpa.split_whitespace().nth(1) {
            // `to_uppercase()` normaliza antes de comparar/guardar, para que
            // "info" e "INFO" caiam sempre na mesma chave do mapa.
            let token_maiusculo = token.to_uppercase();
            if NIVEIS_CONTAINER.contains(&token_maiusculo.as_str()) {
                // API `entry`: busca a chave `token_maiusculo` no mapa e, se
                // ainda não existir, insere `0` antes de devolver a referência
                // mutável ao valor (`or_insert(0)`). O `*` desreferencia essa
                // referência para poder somar `1` no lugar, numa única
                // expressão em vez de "verificar se existe, senão criar, senão
                // incrementar".
                *niveis.entry(token_maiusculo).or_insert(0) += 1;
            }
        }
    }
    niveis
}

// Níveis que `docker logs` pode conter — mescla dos formatos supervisor
// (DEBG, CRIT, ERRO, TRAC) com os formatos de app (DEBUG, INFO, WARN, ERROR).
const NIVEIS_DOCKER: [&str; 12] = [
    "TRACE", "TRAC", "DEBUG", "DEBG", "INFO", "WARN", "WARNING", "ERROR", "ERRO", "CRITICAL",
    "CRIT", "FATAL",
];

// NÚCLEO PURO: conta ocorrências de níveis de log numa string, procurando em
// qualquer token da linha (não apenas numa posição fixa). Funciona com os
// formatos do supervisor ("2026-07-06 09:05:11,722 DEBG ..."), do container
// logs ("2026-07-03 INFO ...") e de apps que logam no formato livre.
fn contar_niveis_docker(conteudo: &str) -> BTreeMap<String, usize> {
    let mut niveis = BTreeMap::new();
    for linha in conteudo.lines() {
        let limpa = remover_ansi(linha);
        // Varre todos os tokens whitespace-delimited em busca de um nível
        // conhecido. Usamos `any` que para no primeiro match (1 nível por
        // linha, mesmo que a linha contenha múltiplas ocorrências).
        if let Some(nivel) = limpa
            .split_whitespace()
            .find(|token| NIVEIS_DOCKER.contains(&token.to_uppercase().as_str()))
        {
            *niveis.entry(nivel.to_uppercase()).or_insert(0) += 1;
        }
    }
    niveis
}

// Monta o bloco de texto (colorido) com os níveis de um container.
// Recebe um `BTreeMap` "cru" (em vez do `struct Contagens` usado por
// `renderizar`) porque aqui só existe uma dimensão de contagem (níveis) — não
// há a segunda categoria "palavras-chave no texto" que o modo `stats` tem.
fn renderizar_container(
    nome: &str,
    status: Option<&str>,
    niveis: &BTreeMap<String, usize>,
) -> String {
    let cabecalho = if let Some(s) = status
        && !s.is_empty()
    {
        format!("📦 {}  ({})", nome.bold(), s.dimmed())
    } else {
        format!("📦 {}", nome.bold())
    };
    let mut saida = format!("{cabecalho}\n");

    // `any`: verdadeiro se existir ao menos uma entrada com contagem > 0;
    // pára na primeira que satisfizer, sem percorrer o mapa inteiro à toa.
    if niveis.values().any(|&valor| valor > 0) {
        saida.push_str("   ");
        let campos: Vec<String> = niveis
            .iter()
            .filter(|&(_, &contagem)| contagem > 0)
            .map(|(nivel, &contagem)| colorir_nivel(nivel, contagem))
            .collect();
        saida.push_str(&campos.join("   "));
        saida.push('\n');
    } else {
        saida.push_str("   (nenhum nível reconhecido nos logs)\n");
    }

    saida
}

// Resultado do núcleo puro: dois mapas de "rótulo -> quantidade".
// `BTreeMap` mantém as chaves ordenadas, o que dá saída determinística.
// `#[derive(Default)]` dá um construtor `Contagens::default()` com mapas vazios;
// `PartialEq` permite comparar com `assert_eq!` nos testes.
#[derive(Default, Debug, PartialEq)]
struct Contagens {
    niveis: BTreeMap<String, usize>,   // coluna de nível do supervisord
    palavras: BTreeMap<String, usize>, // palavras-chave encontradas no texto
}

// NÚCLEO PURO: uma passada por todas as linhas, sem nenhum efeito colateral.
fn contar(conteudo: &str) -> Contagens {
    let mut contagens = Contagens::default();

    // `lines()` itera linha a linha sem alocar cópias (empresta fatias do texto).
    for linha in conteudo.lines() {
        // Coluna supervisord: `split_whitespace()` quebra por espaços e `nth(2)`
        // pega o 3º token (índice 2). Retorna `Option`: `None` se não existir.
        // A "let chain" (`&& ...`) só entra no bloco se o token existir E for um
        // nível conhecido (`contains` sobre o array).
        if let Some(token) = linha.split_whitespace().nth(2)
            && NIVEIS_CONHECIDOS.contains(&token)
        {
            // API `entry`: pega o contador da chave (inserindo 0 se ausente)
            // e o `*... += 1` incrementa o valor no lugar.
            *contagens.niveis.entry(token.to_string()).or_insert(0) += 1;
        }

        // Busca de palavras-chave case-insensitive: comparamos tudo em minúsculo.
        let linha_minuscula = linha.to_lowercase();
        for palavra in PALAVRAS_CHAVE {
            // Conta 1 por LINHA que contém a palavra (não por ocorrência).
            if linha_minuscula.contains(palavra) {
                *contagens.palavras.entry(palavra.to_string()).or_insert(0) += 1;
            }
        }
    }

    contagens
}

// Escolhe a cor conforme a severidade e devolve o campo já formatado.
fn colorir_nivel(nivel: &str, contagem: usize) -> String {
    // O número em negrito; `{}` usa o `Display` que o `.bold()` produz.
    let texto = format!("{nivel} {}", contagem.bold());
    // Comparamos em maiúsculo para casar tanto "ERRO"/"error" quanto "INFO"/"info".
    match nivel.to_uppercase().as_str() {
        "ERROR" | "ERRO" | "CRIT" | "CRITICAL" | "FATAL" => texto.red().to_string(),
        "WARN" | "WARNING" => texto.yellow().to_string(),
        "INFO" => texto.green().to_string(),
        "DEBUG" | "DEBG" => texto.dimmed().to_string(),
        "TRAC" | "TRACE" => texto.cyan().to_string(),
        // `_` é o caso-curinga: qualquer outro rótulo fica sem cor.
        _ => texto,
    }
}

// Monta o bloco de texto (colorido) de um container.
fn renderizar(nome: &str, contagens: &Contagens) -> String {
    let mut saida = format!("📦 {}\n", nome.bold());

    // Só imprime a linha "supervisord" se houver ao menos uma contagem > 0.
    // `any` para na primeira que satisfaz a condição.
    if contagens.niveis.values().any(|&valor| valor > 0) {
        saida.push_str("   supervisord   ");
        // Coleta cada nível não-zero já colorido numa lista de Strings.
        // O padrão `|&(_, &contagem)|` desestrutura a referência da tupla:
        // `_` ignora a chave e `&contagem` copia o número emprestado.
        let campos: Vec<String> = contagens
            .niveis
            .iter()
            .filter(|&(_, &contagem)| contagem > 0)
            .map(|(nivel, &contagem)| colorir_nivel(nivel, contagem))
            .collect();
        // `join` intercala os campos com espaços, sem separador sobrando na ponta.
        saida.push_str(&campos.join("   "));
        saida.push('\n');
    }

    // Mesma lógica para as palavras-chave encontradas no texto.
    if contagens.palavras.values().any(|&valor| valor > 0) {
        saida.push_str("   texto         ");
        let campos: Vec<String> = contagens
            .palavras
            .iter()
            .filter(|&(_, &contagem)| contagem > 0)
            .map(|(palavra, &contagem)| colorir_nivel(palavra, contagem))
            .collect();
        saida.push_str(&campos.join("   "));
        saida.push('\n');
    }

    saida
}

// `#[cfg(test)]`: este módulo só é compilado ao rodar `cargo test`.
#[cfg(test)]
mod tests {
    // Traz para o escopo tudo do módulo pai (incluindo `contar` e `Contagens`).
    use super::*;

    #[test]
    fn conta_niveis_supervisord_e_ignora_continuacoes() {
        // `"\` no fim da linha junta as linhas seguintes sem espaço extra.
        let conteudo = "\
2026-05-08 14:29:39,438 INFO mensagem qualquer
2026-05-08 14:29:39,438 DEBG mensagem com Error no meio
2026-05-08 14:29:39,438 WARN aviso
2026-05-08 14:29:39,438 CRIT falha grave
2026-05-08 14:29:39,438 ERRO erro explícito
2026-05-08 14:29:39,438 TRAC rastreio detalhado
linha de continuação sem timestamp nem nível";

        let contagens = contar(conteudo);

        // Cada nível apareceu exatamente uma vez.
        assert_eq!(contagens.niveis.get("INFO"), Some(&1));
        assert_eq!(contagens.niveis.get("DEBG"), Some(&1));
        assert_eq!(contagens.niveis.get("WARN"), Some(&1));
        assert_eq!(contagens.niveis.get("CRIT"), Some(&1));
        assert_eq!(contagens.niveis.get("ERRO"), Some(&1));
        assert_eq!(contagens.niveis.get("TRAC"), Some(&1));
        // A linha de continuação não adicionou nenhuma chave nova.
        assert_eq!(contagens.niveis.len(), 6);
    }

    #[test]
    fn linha_de_continuacao_nao_conta_nivel() {
        let conteudo = "isso não tem timestamp nem nível válido na posição 3";
        let contagens = contar(conteudo);
        assert!(contagens.niveis.is_empty());
    }

    #[test]
    fn palavra_chave_error_e_case_insensitive() {
        let conteudo = "\
2026-05-08 14:29:39,438 DEBG contém Error no texto
2026-05-08 14:29:39,438 DEBG contém ERROR no texto
2026-05-08 14:29:39,438 DEBG contém error no texto";

        let contagens = contar(conteudo);

        // Error / ERROR / error contam todos para a mesma chave "error".
        assert_eq!(contagens.palavras.get("error"), Some(&3));
    }

    #[test]
    fn conta_uma_vez_por_linha_mesmo_com_multiplas_ocorrencias() {
        let conteudo = "2026-05-08 14:29:39,438 INFO error error error na mesma linha";
        let contagens = contar(conteudo);
        // Três ocorrências na mesma linha = 1 (contamos por linha).
        assert_eq!(contagens.palavras.get("error"), Some(&1));
    }

    #[test]
    fn remove_ansi_preserva_apenas_texto_visivel() {
        // Sequência real emitida por `container logs`: "\x1b[32m INFO\x1b[0m".
        let linha =
            "\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: msg";
        assert_eq!(remover_ansi(linha), "2026-07-03  INFO dev_web: msg");
    }

    #[test]
    fn conta_niveis_container_ignora_codigos_ansi() {
        let conteudo = "\
\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[32m INFO\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: server starting
\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[31mERROR\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: falha ao conectar
\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[33m WARN\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: lento
\u{1b}[2m2026-07-03\u{1b}[0m \u{1b}[35mCRITICAL\u{1b}[0m \u{1b}[2mdev_web\u{1b}[0m: crash iminente";

        let niveis = contar_niveis_container(conteudo);

        assert_eq!(niveis.get("INFO"), Some(&1));
        assert_eq!(niveis.get("ERROR"), Some(&1));
        assert_eq!(niveis.get("WARN"), Some(&1));
        assert_eq!(niveis.get("CRITICAL"), Some(&1));
        assert_eq!(niveis.len(), 4);
    }

    #[test]
    fn conta_niveis_container_ignora_linhas_sem_nivel_conhecido() {
        let conteudo = "mensagem qualquer sem nivel reconhecivel no segundo token";
        let niveis = contar_niveis_container(conteudo);
        assert!(niveis.is_empty());
    }
}
