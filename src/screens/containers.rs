// Tela inicial da TUI: lista de containers com suas contagens por nível,
// mais o painel de tabelas do banco SQLite no rodapé.
//
// Navegação: ↑/↓ seleciona, Enter vai para os níveis do container, q sai.

use std::collections::BTreeMap;

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use rusqlite::Connection;

use crate::screens::app_types::AppTypeScreen;
use crate::screens::lines::carregar_todas_linhas;
use crate::screens::{Screen, ScreenAction};

// --- Modelo de dados -------------------------------------------------------

/// Dados de um container já resumidos, prontos para exibição: nome, contagem
/// de logs por nível (ordenada por nome do nível, graças ao `BTreeMap`) e o
/// texto de uptime que veio do banco (pode ser vazio se ainda não foi coletado).
struct ContainerInfo {
    name: String,
    niveles: BTreeMap<String, i64>,
    uptime: String,
}

/// Metadados de uma tabela do SQLite: nome, quantidade de linhas e lista de
/// colunas (cada uma no formato "nome tipo"). Carregado uma vez ao abrir a TUI
/// e exibido no painel inferior da tela de containers.
struct TableInfo {
    name: String,
    row_count: i64,
    colunas: Vec<String>,
}

// --- ContainerScreen -------------------------------------------------------

pub(crate) struct ContainerScreen {
    containers: Vec<ContainerInfo>,
    selected: usize,
    offset: usize,
    tabelas: Vec<TableInfo>,
}

impl ContainerScreen {
    /// Cria a tela carregando containers e metadados do banco.
    pub(crate) fn new(conn: &Connection) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            containers: carregar_containers(conn),
            selected: 0,
            offset: 0,
            tabelas: carregar_tabelas(conn),
        })
    }
}

impl Screen for ContainerScreen {
    fn handle_key(&mut self, key: KeyCode, conn: &Connection) -> ScreenAction {
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                ScreenAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.containers.len().saturating_sub(1);
                self.selected = self.selected.saturating_add(1).min(max);
                ScreenAction::None
            }
            KeyCode::Enter => {
                if self.containers.is_empty() {
                    return ScreenAction::None;
                }
                let c = &self.containers[self.selected];
                let nome = c.name.clone();
                let linhas = carregar_todas_linhas(conn, &nome);
                ScreenAction::Push(Box::new(AppTypeScreen::new(nome, linhas)))
            }
            KeyCode::Char('q') => ScreenAction::Quit,
            _ => ScreenAction::None,
        }
    }

    fn handle_click(&mut self, row: u16, _col: u16, conn: &Connection) -> ScreenAction {
        let Ok((_, h)) = crossterm::terminal::size() else {
            return ScreenAction::None;
        };
        let tabelas_h = (self.tabelas.len() as u16 + 3).min(10);
        let list_h = h.saturating_sub(tabelas_h + 1);
        if row >= 1 && row + 1 < list_h {
            let index = (row as usize).saturating_sub(1) + self.offset;
            if index < self.containers.len() {
                self.selected = index;
                return self.handle_key(KeyCode::Enter, conn);
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame) {
        let items: Vec<ListItem> = self
            .containers
            .iter()
            .map(|c| {
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
                ListItem::new(linha)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Containers ({}) ", self.containers.len())),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );

        let tabelas_height = (self.tabelas.len() as u16 + 3).min(10);
        let area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),
                Constraint::Length(tabelas_height),
                Constraint::Length(1),
            ])
            .split(f.area());

        let visiveis = (area[0].height as usize).saturating_sub(2);
        self.offset = self
            .offset
            .min(self.containers.len().saturating_sub(visiveis.max(1)));
        if self.selected < self.offset {
            self.offset = self.selected;
        }
        if self.selected >= self.offset + visiveis {
            self.offset = self.selected.saturating_add(1).saturating_sub(visiveis);
        }

        let mut state = ListState::default()
            .with_selected(Some(self.selected))
            .with_offset(self.offset);
        f.render_stateful_widget(list, area[0], &mut state);
        renderizar_tabelas(f, &self.tabelas, area[1]);

        let help = Paragraph::new(format!(
            "  ↑/↓ navegar  Enter:ver níveis  q:sair   ({})",
            self.containers.len()
        ))
        .style(Style::default().fg(Color::DarkGray));
        f.render_widget(help, area[2]);
    }
}

// --- Funções auxiliares ----------------------------------------------------

/// Desenha o painel de tabelas do banco SQLite.
fn renderizar_tabelas(f: &mut Frame, tabelas: &[TableInfo], area: ratatui::prelude::Rect) {
    let mut linhas = Vec::new();
    for t in tabelas {
        let cols = if t.colunas.is_empty() {
            String::new()
        } else {
            format!(" [{}]", t.colunas.join(", "))
        };
        linhas.push(format!("  {:20} {:>8} linhas{}", t.name, t.row_count, cols));
    }
    let text = Paragraph::new(linhas.join("\n")).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Database Tables ({}) ", tabelas.len())),
    );
    f.render_widget(text, area);
}

/// Carrega todos os containers e suas contagens agregadas do banco.
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

/// Carrega os metadados de todas as tabelas do banco.
fn carregar_tabelas(conn: &Connection) -> Vec<TableInfo> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap();
    let nomes: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .flatten()
        .collect();

    let mut tabelas = Vec::new();
    for nome in &nomes {
        let row_count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM \"{nome}\""), [], |r| {
                r.get(0)
            })
            .unwrap_or(0);

        let mut colunas = Vec::new();
        if let Ok(mut s) = conn.prepare(&format!("PRAGMA table_info(\"{nome}\")")) {
            for linha in s
                .query_map([], |r| {
                    let nome_col: String = r.get(1)?;
                    let tipo: String = r.get(2)?;
                    Ok(format!("{nome_col} {tipo}"))
                })
                .unwrap()
                .flatten()
            {
                colunas.push(linha);
            }
        }

        tabelas.push(TableInfo {
            name: nome.clone(),
            row_count,
            colunas,
        });
    }
    tabelas
}
