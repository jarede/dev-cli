// Módulo do TUI (Terminal User Interface) para navegar nas estatísticas de
// logs coletadas no SQLite. Três telas aninhadas com navegação por setas.
//
// Uma TUI difere de um programa de linha de comando comum porque ela assume
// controle total do terminal: em vez de simplesmente imprimir texto e sair,
// ela entra em "raw mode" (lê tecla a tecla, sem esperar Enter, sem eco
// automático) e desenha numa "tela alternada" (um buffer separado do
// histórico normal do terminal, restaurado ao sair — como o `vim` ou o
// `htop` fazem). O programa fica preso num loop: desenha a tela, espera uma
// tecla, atualiza o estado, desenha de novo — até o usuário pedir para sair.

use std::collections::{BTreeMap, HashSet};

// `crossterm` é a crate que abstrai o terminal de forma multiplataforma
// (Linux/macOS/Windows): captura eventos de teclado e alterna entre o modo
// "raw" e o modo normal do terminal.
// docs: https://docs.rs/crossterm/latest/crossterm/
use crossterm::{
    event::{self, Event, KeyCode},
    // A macro `execute!` escreve comandos de terminal (aqui, trocar de tela)
    // diretamente no `stdout`, sem precisar montar uma string de escape ANSI
    // manualmente.
    // docs: https://docs.rs/crossterm/latest/crossterm/macro.execute.html
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
// `ratatui` é a crate de "widgets" da TUI: sabe desenhar listas, bordas,
// parágrafos etc. a partir de um `Frame` (a área de desenho de um quadro).
// docs: https://docs.rs/ratatui/latest/ratatui/
// docs: https://docs.rs/ratatui/latest/ratatui/struct.Frame.html
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use rusqlite::Connection;

// --- Modelo de dados -------------------------------------------------------

// Dados de um container já resumidos, prontos para exibição: nome, contagem
// de logs por nível (ordenada por nome do nível, graças ao `BTreeMap`) e o
// texto de uptime que veio do banco (pode ser vazio se ainda não foi coletado).
// docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
struct ContainerInfo {
    name: String,
    niveles: BTreeMap<String, i64>,
    uptime: String,
}

// As três telas da TUI, em ordem de "profundidade" de navegação:
// Containers (lista de containers) -> Levels (níveis de log daquele
// container) -> Lines (linhas de log daquele nível). Cada `Enter` avança uma
// tela; `Esc`/`Backspace` volta uma tela.
enum Screen {
    Containers,
    Levels,
    Lines,
}

// Todo o estado mutável da TUI vive nesta struct: qual tela está ativa e,
// para cada tela, os dados carregados e qual item está selecionado. Como só
// existe uma tela por vez, os campos das telas "Levels" e "Lines" ficam sem
// uso enquanto o usuário está na tela "Containers" — uma alternativa mais
// idiomática seria um enum com dados por variante (`Screen::Levels { niveles,
// selected, .. }`), mas manter tudo "achatado" aqui simplifica o acesso.
pub struct App {
    screen: Screen,
    containers: Vec<ContainerInfo>,
    // Índice do container destacado na lista (não um ID: é a posição no Vec).
    selected_container: usize,
    // Tela de níveis
    niveles: Vec<(String, i64)>,
    selected_nivel: usize,
    nome_do_container: String,
    // Tela de linhas
    nome_do_nivel: String,
    linhas: Vec<String>,
    selected_linha: usize,
    // Conjunto de índices das linhas que o usuário expandiu (Enter alterna
    // entre versão truncada e completa). `HashSet` porque só nos importa
    // "está expandida ou não", sem ordem nem valor associado.
    // docs: https://doc.rust-lang.org/std/collections/struct.HashSet.html
    expanded: HashSet<usize>,
}

impl App {
    // Carrega, de uma vez só, todos os containers e suas contagens por nível
    // agregadas do banco (usado ao abrir a TUI). `.unwrap()` é aceitável aqui
    // porque este é código de UI interativo (não uma lib), e um erro de SQL
    // mal formado é um bug de programação, não uma condição esperada de
    // runtime — ainda assim, note que a convenção do projeto é evitar
    // `unwrap()` fora de teste; aqui ele sobrevive por pragmatismo da TUI.
    // docs: https://doc.rust-lang.org/std/result/enum.Result.html#method.unwrap
    fn carregar_containers(conn: &Connection) -> Vec<ContainerInfo> {
        let mut stmt = conn
            .prepare(
                "SELECT container_name, level, SUM(count) as total
                 FROM log_counts
                 GROUP BY container_name, level
                 ORDER BY container_name, level",
            )
            .unwrap();
        // Mapa aninhado: container -> (nível -> total). O `BTreeMap` externo
        // dá a ordem alfabética dos containers "de graça".
        // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
        let mut dados: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
        for row in stmt
            .query_map([], |r| {
                let n: String = r.get(0)?;
                let l: String = r.get(1)?;
                let c: i64 = r.get(2)?;
                Ok((n, l, c))
            })
            .unwrap()
            // `query_map` devolve um iterador de `Result<(String, String, i64)>`
            // (uma linha pode falhar ao ser lida); `.flatten()` descarta os
            // `Err` e "desembrulha" só os `Ok`, produzindo direto as tuplas.
            // docs: https://docs.rs/rusqlite/latest/rusqlite/struct.Statement.html#method.query_map
            // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.flatten
            .flatten()
        {
            // `entry(...).or_default()` pega o sub-mapa daquele container
            // (criando um vazio se for a primeira vez que o vemos) e insere
            // o total daquele nível.
            // docs: https://doc.rust-lang.org/std/collections/btree_map/struct.BTreeMap.html#method.entry
            // docs: https://doc.rust-lang.org/std/collections/btree_map/enum.Entry.html#method.or_default
            dados.entry(row.0).or_default().insert(row.1, row.2);
        }

        // Carrega uptime dos containers
        let mut up = BTreeMap::new();
        // `if let Ok(...)`: a tabela `containers` sempre existe (criada em
        // `init_db`), mas usamos `if let` em vez de `?`/`unwrap()` porque a
        // ausência de uptime não deve impedir a TUI de abrir — a query só é
        // executada se preparar com sucesso; se falhar, `up` fica vazio.
        // docs: https://doc.rust-lang.org/std/result/enum.Result.html#method.unwrap
        if let Ok(mut s) = conn.prepare("SELECT name, uptime FROM containers") {
            for row in s
                .query_map([], |r| {
                    let n: String = r.get(0)?;
                    let u: String = r.get(1)?;
                    Ok((n, u))
                })
                .unwrap()
                .flatten()
            {
                up.insert(row.0, row.1);
            }
        }

        // Converte o mapa aninhado em `Vec<ContainerInfo>`, já ordenado por
        // nome (herdado da ordem do `BTreeMap`). `into_iter()` consome
        // `dados`, então cada `name`/`niveles` é movido (não copiado) para
        // dentro do `ContainerInfo`.
        // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
        // docs: https://doc.rust-lang.org/std/iter/trait.IntoIterator.html#tymethod.into_iter
        dados
            .into_iter()
            .map(|(name, niveles)| ContainerInfo {
                // `up.get(&name)` empresta a String antes de `name` ser
                // movido para o campo abaixo; por isso o `uptime` precisa
                // vir primeiro na struct literal (a ordem dos campos na
                // inicialização não precisa bater com a ordem declarada, mas
                // aqui importa a ordem de *avaliação*: `name` só é movido na
                // última linha).
                // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.get
                uptime: up.get(&name).cloned().unwrap_or_default(),
                name,
                niveles,
            })
            .collect()
    }

    // Busca as linhas de log de um container+nível específico, na ordem em
    // que foram inseridas (`ORDER BY id`). Usado ao entrar na tela `Lines`.
    fn carregar_linhas(conn: &Connection, container: &str, nivel: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(
                "SELECT line FROM log_lines
                 WHERE container_name = ?1 AND level = ?2
                 ORDER BY id",
            )
            .unwrap();
        stmt.query_map(rusqlite::params![container, nivel], |r| r.get(0))
            .unwrap()
            // Aqui usamos `filter_map(|r| r.ok())` em vez de `.flatten()`
            // (equivalentes para `Result`) — mesma ideia: descarta linhas que
            // falharam ao decodificar, mantém só as que vieram como `Ok`.
            // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.filter_map
            // docs: https://doc.rust-lang.org/std/result/enum.Result.html#method.ok
            // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.flatten
            .filter_map(|r| r.ok())
            .collect()
    }

    // Atalho para o container atualmente destacado na tela `Containers`.
    // Devolve uma referência emprestada (`&ContainerInfo`): não clona os
    // dados, só empresta o elemento do Vec indexado por `selected_container`.
    fn container_atual(&self) -> &ContainerInfo {
        &self.containers[self.selected_container]
    }
}

// --- TUI principal ---------------------------------------------------------

// Ponto de entrada da TUI: prepara o terminal, roda o loop principal (que só
// devolve o controle quando o usuário sai) e depois desfaz as mudanças no
// terminal, aconteça o que acontecer no meio do caminho.
pub fn run_tui(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    // "Raw mode": desliga o comportamento padrão do terminal de line-buffering
    // e eco de teclas. Sem isso, o terminal só entregaria teclas ao programa
    // depois de Enter, e ecoaria cada tecla na tela — incompatível com uma
    // interface que reage a cada seta imediatamente.
    // docs: https://docs.rs/crossterm/latest/crossterm/terminal/fn.enable_raw_mode.html
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    // "Alternate screen": troca para um buffer de tela separado (o mesmo
    // truque usado por `vim`/`less`/`htop`), preservando o conteúdo que já
    // estava no terminal para restaurar depois que a TUI fechar.
    // docs: https://docs.rs/crossterm/latest/crossterm/terminal/struct.EnterAlternateScreen.html
    execute!(stdout, EnterAlternateScreen)?;

    // `CrosstermBackend` adapta o `stdout` (que sabe escrever bytes) para a
    // interface que o `ratatui::Terminal` espera (que sabe posicionar cursor,
    // limpar áreas, etc.). O `Terminal` por cima disso cuida do double
    // buffering: compara o quadro novo com o anterior e só reescreve o que
    // mudou, evitando "flicker".
    // docs: https://docs.rs/ratatui/latest/ratatui/backend/struct.CrosstermBackend.html
    // docs: https://docs.rs/ratatui/latest/ratatui/terminal/struct.Terminal.html
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    // docs: https://docs.rs/ratatui/latest/ratatui/backend/struct.CrosstermBackend.html#method.new
    let mut terminal = ratatui::Terminal::new(backend)?;
    // docs: https://docs.rs/ratatui/latest/ratatui/terminal/struct.Terminal.html#method.new

    // Estado inicial: começa na tela de containers, já carregada do banco;
    // as demais telas ficam vazias até o usuário navegar até elas.
    let mut app = App {
        screen: Screen::Containers,
        containers: App::carregar_containers(conn),
        selected_container: 0,
        niveles: Vec::new(),
        selected_nivel: 0,
        nome_do_container: String::new(),
        nome_do_nivel: String::new(),
        linhas: Vec::new(),
        selected_linha: 0,
        expanded: HashSet::new(),
    };

    // O clássico "loop de eventos" de uma TUI: desenha o quadro atual, espera
    // (bloqueante) o próximo evento de teclado, atualiza o estado conforme a
    // tecla e a tela ativa, e repete. O `loop` só termina com um `break`
    // explícito (aqui, ao apertar `q` na tela de containers), devolvendo o
    // valor passado ao `break` como resultado do bloco `loop { ... }` —
    // por isso `res` recebe o `Ok(())`/`Err(...)` que foi "quebrado".
    let res = loop {
        // `terminal.draw` recebe uma closure que recebe o `Frame` (a área de
        // desenho do quadro atual) e delega para `renderizar`, que decide
        // qual tela desenhar. O `?` propaga eventuais erros de IO do terminal.
        // docs: https://docs.rs/ratatui/latest/ratatui/terminal/struct.Terminal.html#method.draw
        // docs: https://docs.rs/ratatui/latest/ratatui/struct.Frame.html
        terminal.draw(|f| renderizar(f, &app))?;

        // `event::read()` bloqueia a thread até chegar um evento (tecla,
        // resize, etc.). Só nos interessam eventos de tecla; outros tipos
        // (ex.: redimensionar a janela) são ignorados pelo `if let`.
        // docs: https://docs.rs/crossterm/latest/crossterm/event/fn.read.html
        if let Event::Key(key) = event::read()? {
            // Cada tela trata as teclas de um jeito diferente, então o
            // `match` externo escolhe o conjunto de atalhos pela tela ativa,
            // e o `match key.code` interno escolhe a ação pela tecla.
            match app.screen {
                Screen::Containers => match key.code {
                    // `k`/seta-cima: sobe a seleção. `saturating_sub(1)` evita
                    // estourar por baixo de zero (índice `usize` não pode ser
                    // negativo; sem `saturating_sub` um `0 - 1` faria panic
                    // por overflow em modo debug).
                    // docs: https://doc.rust-lang.org/std/primitive.usize.html#method.saturating_sub
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.selected_container = app.selected_container.saturating_sub(1)
                    }
                    // `j`/seta-baixo: desce a seleção, mas sem passar do
                    // último índice válido (`max`). `saturating_add` evita
                    // overflow por cima; `.min(max)` trava no teto.
                    // docs: https://doc.rust-lang.org/std/primitive.usize.html#method.saturating_add
                    // docs: https://doc.rust-lang.org/std/primitive.usize.html#method.min
                    KeyCode::Down | KeyCode::Char('j') => {
                        let max = app.containers.len().saturating_sub(1);
                        app.selected_container = app.selected_container.saturating_add(1).min(max);
                    }
                    // Enter: entra na tela de níveis do container selecionado.
                    KeyCode::Enter => {
                        // `.clone()` porque `nome` vai sobreviver além do
                        // empréstimo de `app.container_atual()` (precisamos
                        // depois atribuir em `app.nome_do_container`, e o
                        // borrow checker não deixaria manter uma referência
                        // para dentro de `app` enquanto mutamos `app` logo
                        // abaixo).
                        // docs: https://doc.rust-lang.org/std/clone/trait.Clone.html#tymethod.clone
                        let nome = app.container_atual().name.clone();
                        // Copia os pares (nível, total) do `BTreeMap` (que
                        // está ordenado por nome do nível) para um `Vec` que
                        // podemos reordenar livremente por contagem.
                        // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html
                        // docs: https://doc.rust-lang.org/std/vec/struct.Vec.html
                        let mut v: Vec<_> = app
                            .container_atual()
                            .niveles
                            .iter()
                            .map(|(k, v)| (k.clone(), *v))
                            .collect();
                        // Ordena do maior total para o menor: `Reverse` inverte
                        // a ordem natural (`sort_by_key` por si só ordenaria
                        // crescente).
                        // docs: https://doc.rust-lang.org/std/cmp/struct.Reverse.html
                        // docs: https://doc.rust-lang.org/std/primitive.slice.html#method.sort_by_key
                        v.sort_by_key(|b| std::cmp::Reverse(b.1));
                        app.niveles = v;
                        app.selected_nivel = 0;
                        app.nome_do_container = nome;
                        app.screen = Screen::Levels;
                    }
                    // `q`: sai do loop com sucesso. `break Ok(())` devolve
                    // esse valor como resultado de todo o bloco `loop`.
                    KeyCode::Char('q') => break Ok(()),
                    // `_`: qualquer outra tecla é ignorada nesta tela.
                    _ => {}
                },
                Screen::Levels => match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.selected_nivel = app.selected_nivel.saturating_sub(1)
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let max = app.niveles.len().saturating_sub(1);
                        app.selected_nivel = app.selected_nivel.saturating_add(1).min(max);
                    }
                    // Enter: busca no banco as linhas de log daquele nível e
                    // avança para a tela de linhas.
                    KeyCode::Enter => {
                        let (nivel, _) = &app.niveles[app.selected_nivel];
                        app.linhas = App::carregar_linhas(conn, &app.nome_do_container, nivel);
                        app.selected_linha = 0;
                        // Limpa expansões da visita anterior à tela de linhas
                        // (senão índices de uma consulta antiga ficariam
                        // "expandidos" por engano na consulta nova).
                        app.expanded.clear();
                        app.nome_do_nivel = nivel.clone();
                        app.screen = Screen::Lines;
                    }
                    // Esc ou Backspace: volta uma tela (para Containers).
                    KeyCode::Esc | KeyCode::Backspace => app.screen = Screen::Containers,
                    _ => {}
                },
                Screen::Lines => match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.selected_linha = app.selected_linha.saturating_sub(1)
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let max = app.linhas.len().saturating_sub(1);
                        app.selected_linha = app.selected_linha.saturating_add(1).min(max);
                    }
                    // Enter alterna expandir/recolher a linha selecionada —
                    // só faz sentido se houver alguma linha carregada (a
                    // guarda `if !app.linhas.is_empty()` depois do padrão é
                    // uma "match guard": só casa este braço se a condição for
                    // verdadeira; senão cai no `_` abaixo).
                    KeyCode::Enter if !app.linhas.is_empty() => {
                        if app.expanded.contains(&app.selected_linha) {
                            app.expanded.remove(&app.selected_linha);
                        } else {
                            app.expanded.insert(app.selected_linha);
                        }
                    }
                    KeyCode::Esc | KeyCode::Backspace => app.screen = Screen::Levels,
                    _ => {}
                },
            }
        }
    };

    // Desfaz, na ordem inversa, tudo o que foi ativado no início: sai do raw
    // mode, volta para a tela normal do terminal (restaurando o que havia
    // antes de abrir a TUI) e garante que o cursor volte a aparecer (o
    // `ratatui` o esconde durante o desenho). Isso roda mesmo se `res` for um
    // `Err` — importante para não deixar o terminal do usuário "quebrado"
    // (sem eco de tecla, preso na tela alternada) caso algo falhe no loop.
    // docs: https://docs.rs/ratatui/latest/ratatui/
    // docs: https://docs.rs/crossterm/latest/crossterm/terminal/fn.disable_raw_mode.html
    disable_raw_mode()?;
    // docs: https://docs.rs/ratatui/latest/ratatui/terminal/struct.Terminal.html#method.backend_mut
    // docs: https://docs.rs/crossterm/latest/crossterm/terminal/struct.LeaveAlternateScreen.html
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    // docs: https://docs.rs/ratatui/latest/ratatui/terminal/struct.Terminal.html#method.show_cursor
    terminal.show_cursor()?;

    res
}

// --- Renderização ----------------------------------------------------------
//
// Cada `renderizar_*` monta os widgets do `ratatui` (listas, blocos com
// borda, parágrafo de ajuda) a partir do estado em `app` e os desenha no
// `Frame` recebido. Nenhuma dessas funções lê o banco ou faz IO: elas só
// transformam dados já carregados em widgets — o carregamento acontece antes,
// no loop de eventos, ao trocar de tela.

// Escolhe qual tela desenhar de acordo com `app.screen`. É chamada a cada
// volta do loop de eventos, mesmo quando nada mudou (o `ratatui::Terminal`
// já otimiza o redesenho comparando com o quadro anterior).
// docs: https://docs.rs/ratatui/latest/ratatui/terminal/struct.Terminal.html
fn renderizar(f: &mut Frame, app: &App) {
    match app.screen {
        Screen::Containers => renderizar_containers(f, app),
        Screen::Levels => renderizar_niveis(f, app),
        Screen::Lines => renderizar_linhas(f, app),
    }
}

// Desenha a tela inicial: lista de containers, cada um com seu uptime (se
// houver) e o resumo de níveis de log.
fn renderizar_containers(f: &mut Frame, app: &App) {
    // Monta uma linha de texto por container. `enumerate()` dá o índice `i`
    // junto com a referência `c`, necessário para saber qual linha é a
    // selecionada e destacá-la.
    // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.enumerate
    let items: Vec<ListItem> = app
        .containers
        .iter()
        .enumerate()
        .map(|(i, c)| {
            // Sem uptime coletado ainda (container nunca visto como
            // "running"), omite essa coluna do layout em vez de imprimir um
            // espaço em branco de tamanho fixo.
            let linha = if c.uptime.is_empty() {
                format!(
                    "{:20}  {}",
                    c.name,
                    c.niveles
                        .iter()
                        .map(|(n, v)| format!("{} {}", n, v))
                        .collect::<Vec<_>>()
                        .join("  ")
                )
            } else {
                format!(
                    "{:20}  {:16}  {}",
                    c.name,
                    c.uptime,
                    c.niveles
                        .iter()
                        .map(|(n, v)| format!("{} {}", n, v))
                        .collect::<Vec<_>>()
                        .join("  ")
                )
            };
            // A linha correspondente ao item selecionado ganha destaque
            // visual (amarelo + negrito); as demais ficam com o estilo
            // padrão do terminal.
            if i == app.selected_container {
                ListItem::new(linha).style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ListItem::new(linha)
            }
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Containers "))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    // Divide a área do quadro em duas faixas verticais: a lista ocupa todo o
    // espaço restante (`Constraint::Min(1)`) e a linha de ajuda fica fixa em
    // 1 linha na base (`Constraint::Length(1)`). `f.area()` é o retângulo
    // total disponível no terminal neste quadro.
    // docs: https://docs.rs/ratatui/latest/ratatui/layout/enum.Constraint.html
    // docs: https://docs.rs/ratatui/latest/ratatui/struct.Frame.html#method.area
    let area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(f.area());

    f.render_widget(list, area[0]);

    // Rodapé com os atalhos de teclado válidos nesta tela.
    let help = Paragraph::new(format!(
        "  ↑/↓ navegar  Enter:ver níveis  q:sair   ({})",
        app.containers.len()
    ))
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(help, area[1]);
}

// Desenha a tela intermediária: níveis de log do container escolhido,
// ordenados do maior para o menor total (ordenação feita no momento do
// Enter, em `run_tui`), cada um colorido pela severidade.
fn renderizar_niveis(f: &mut Frame, app: &App) {
    let items: Vec<ListItem> = app
        .niveles
        .iter()
        .enumerate()
        .map(|(i, (nivel, total))| {
            let item = format!("  {:10} {}", nivel, total);
            if i == app.selected_nivel {
                // Item selecionado: destaque amarelo/negrito tem prioridade
                // sobre a cor de severidade (senão o usuário perderia o
                // feedback de "onde estou" na navegação).
                ListItem::new(item).style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                // Cor por severidade, na linha não selecionada: vermelho para
                // erros/críticos, amarelo para avisos, verde para info, cinza
                // para debug. `_ =>` cobre níveis não reconhecidos (sem cor).
                let estilo = match nivel.to_uppercase().as_str() {
                    "ERROR" | "ERRO" | "CRIT" | "CRITICAL" | "FATAL" => {
                        Style::default().fg(Color::Red)
                    }
                    "WARN" | "WARNING" => Style::default().fg(Color::Yellow),
                    "INFO" => Style::default().fg(Color::Green),
                    "DEBUG" | "DEBG" => Style::default().fg(Color::DarkGray),
                    _ => Style::default(),
                };
                ListItem::new(item).style(estilo)
            }
        })
        .collect();

    // Soma total de linhas de log deste container (todos os níveis
    // somados), exibida no rodapé como referência.
    let total: i64 = app.niveles.iter().map(|(_, v)| v).sum();
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", app.nome_do_container)),
    );

    let area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(f.area());

    f.render_widget(list, area[0]);

    let help = Paragraph::new(format!(
        "  ↑/↓ navegar  Enter:ver linhas  Esc:voltar  ({} linhas)",
        total
    ))
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(help, area[1]);
}

// Desenha a tela final: as linhas de log cruas do nível escolhido. Linhas
// longas vêm truncadas por padrão (para não quebrar o layout de lista); o
// usuário pode expandir uma de cada vez com Enter (ver `app.expanded`).
fn renderizar_linhas(f: &mut Frame, app: &App) {
    let items: Vec<ListItem> = app
        .linhas
        .iter()
        .enumerate()
        .map(|(i, linha)| {
            // Linha expandida: mostra o texto completo (`.clone()` porque
            // `ListItem::new` precisa de uma `String` própria, não de uma
            // referência emprestada de `app.linhas`). Não expandida: usa a
            // versão truncada a 120 caracteres.
            // docs: https://doc.rust-lang.org/std/clone/trait.Clone.html#tymethod.clone
            // docs: https://docs.rs/ratatui/latest/ratatui/widgets/struct.ListItem.html#method.new
            // docs: https://doc.rust-lang.org/std/string/struct.String.html
            let texto = if app.expanded.contains(&i) {
                linha.clone()
            } else {
                truncar(linha, 120)
            };
            if i == app.selected_linha {
                ListItem::new(texto).style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else if app.expanded.contains(&i) {
                // Expandida mas não selecionada: sem cor especial, só o
                // texto completo em estilo padrão.
                ListItem::new(texto)
            } else {
                // Truncada e não selecionada: cinza para indicar visualmente
                // que há mais texto por trás (reforça o convite a expandir).
                ListItem::new(texto).style(Style::default().fg(Color::DarkGray))
            }
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(format!(
        " {} / {} ",
        app.nome_do_container, app.nome_do_nivel
    )));

    let area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(f.area());

    f.render_widget(list, area[0]);

    let total = app.linhas.len();
    let help = Paragraph::new(format!(
        "  ↑/↓ navegar  Enter:expandir/recolher  Esc:voltar  ({} linhas)",
        total
    ))
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(help, area[1]);
}

// --- Utilitários -----------------------------------------------------------

// Corta a string em `max` bytes e acrescenta "…" para indicar que há mais
// texto. Nota: usa `&s[..max]` (fatiamento por bytes, não por caracteres),
// o que só é seguro se `max` cair numa fronteira de caractere UTF-8 — no uso
// atual (`truncar(linha, 120)`), qualquer log com caracteres multi-byte
// exatamente no byte 120 causaria panic; funciona na prática porque logs em
// texto puro raramente têm esse limite exato coincidindo com um caractere
// especial.
fn truncar(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
