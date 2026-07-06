// Módulo do TUI (Terminal User Interface) para navegar nas estatísticas de
// logs coletadas no SQLite. Três telas aninhadas com navegação por setas.

use std::collections::{BTreeMap, HashSet};

use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use rusqlite::Connection;

// --- Modelo de dados -------------------------------------------------------

struct ContainerInfo {
    name: String,
    niveles: BTreeMap<String, i64>,
    uptime: String,
}

enum Screen {
    Containers,
    Levels,
    Lines,
}

pub struct App {
    screen: Screen,
    containers: Vec<ContainerInfo>,
    selected_container: usize,
    // Tela de níveis
    niveles: Vec<(String, i64)>,
    selected_nivel: usize,
    nome_do_container: String,
    // Tela de linhas
    nome_do_nivel: String,
    linhas: Vec<String>,
    selected_linha: usize,
    expanded: HashSet<usize>,
}

impl App {
    fn carregar_containers(conn: &Connection) -> Vec<ContainerInfo> {
        let mut stmt = conn
            .prepare(
                "SELECT container_name, level, SUM(count) as total
                 FROM log_counts
                 GROUP BY container_name, level
                 ORDER BY container_name, level",
            )
            .unwrap();
        let mut dados: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
        for row in stmt
            .query_map([], |r| {
                let n: String = r.get(0)?;
                let l: String = r.get(1)?;
                let c: i64 = r.get(2)?;
                Ok((n, l, c))
            })
            .unwrap()
            .flatten()
        {
            dados.entry(row.0).or_default().insert(row.1, row.2);
        }

        // Carrega uptime dos containers
        let mut up = BTreeMap::new();
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

        dados
            .into_iter()
            .map(|(name, niveles)| ContainerInfo {
                uptime: up.get(&name).cloned().unwrap_or_default(),
                name,
                niveles,
            })
            .collect()
    }

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
            .filter_map(|r| r.ok())
            .collect()
    }

    fn container_atual(&self) -> &ContainerInfo {
        &self.containers[self.selected_container]
    }
}

// --- TUI principal ---------------------------------------------------------

pub fn run_tui(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

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

    let res = loop {
        terminal.draw(|f| renderizar(f, &app))?;

        if let Event::Key(key) = event::read()? {
            match app.screen {
                Screen::Containers => match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.selected_container = app.selected_container.saturating_sub(1)
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        let max = app.containers.len().saturating_sub(1);
                        app.selected_container = app.selected_container.saturating_add(1).min(max);
                    }
                    KeyCode::Enter => {
                        let nome = app.container_atual().name.clone();
                        let mut v: Vec<_> = app
                            .container_atual()
                            .niveles
                            .iter()
                            .map(|(k, v)| (k.clone(), *v))
                            .collect();
                        v.sort_by_key(|b| std::cmp::Reverse(b.1));
                        app.niveles = v;
                        app.selected_nivel = 0;
                        app.nome_do_container = nome;
                        app.screen = Screen::Levels;
                    }
                    KeyCode::Char('q') => break Ok(()),
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
                    KeyCode::Enter => {
                        let (nivel, _) = &app.niveles[app.selected_nivel];
                        app.linhas = App::carregar_linhas(conn, &app.nome_do_container, nivel);
                        app.selected_linha = 0;
                        app.expanded.clear();
                        app.nome_do_nivel = nivel.clone();
                        app.screen = Screen::Lines;
                    }
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

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    res
}

// --- Renderização ----------------------------------------------------------

fn renderizar(f: &mut Frame, app: &App) {
    match app.screen {
        Screen::Containers => renderizar_containers(f, app),
        Screen::Levels => renderizar_niveis(f, app),
        Screen::Lines => renderizar_linhas(f, app),
    }
}

fn renderizar_containers(f: &mut Frame, app: &App) {
    let items: Vec<ListItem> = app
        .containers
        .iter()
        .enumerate()
        .map(|(i, c)| {
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

    let area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(f.area());

    f.render_widget(list, area[0]);

    let help = Paragraph::new(format!(
        "  ↑/↓ navegar  Enter:ver níveis  q:sair   ({})",
        app.containers.len()
    ))
    .style(Style::default().fg(Color::DarkGray));
    f.render_widget(help, area[1]);
}

fn renderizar_niveis(f: &mut Frame, app: &App) {
    let items: Vec<ListItem> = app
        .niveles
        .iter()
        .enumerate()
        .map(|(i, (nivel, total))| {
            let item = format!("  {:10} {}", nivel, total);
            if i == app.selected_nivel {
                ListItem::new(item).style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
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

fn renderizar_linhas(f: &mut Frame, app: &App) {
    let items: Vec<ListItem> = app
        .linhas
        .iter()
        .enumerate()
        .map(|(i, linha)| {
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
                ListItem::new(texto)
            } else {
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

fn truncar(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
