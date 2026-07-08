// Tela intermediária da TUI: níveis de log de um container específico
// (ou de um tipo de app dentro do container), ordenados do maior para o
// menor total, cada um colorido por severidade.
//
// Quando entra via AppTypeScreen, recebe `lines_by_level` pré-carregadas
// para evitar uma segunda consulta ao banco. Quando entra direto de
// ContainerScreen (modo sem detecção de app), carrega do banco.
//
// Navegação: ↑/↓ seleciona, Enter vai para as linhas daquele nível,
// Esc volta para a tela anterior.

use std::collections::BTreeMap;

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use rusqlite::Connection;

use crate::screens::lines::{carregar_linhas, LinesScreen};
use crate::screens::{Screen, ScreenAction};

pub(crate) struct LevelsScreen {
    pub(crate) nome_do_container: String,
    niveles: Vec<(String, i64)>,
    /// Quando `Some`, as linhas de cada nível já estão carregadas em memória
    /// (vindas da análise de AppTypeScreen). Quando `None`, busca no banco
    /// linha a linha ao entrar num nível (comportamento original).
    lines_by_level: Option<BTreeMap<String, Vec<String>>>,
    selected: usize,
    offset: usize,
}

impl LevelsScreen {
    pub(crate) fn new(
        nome: String,
        niveles: Vec<(String, i64)>,
        lines_by_level: Option<BTreeMap<String, Vec<String>>>,
    ) -> Self {
        Self {
            nome_do_container: nome,
            niveles,
            lines_by_level,
            selected: 0,
            offset: 0,
        }
    }
}

impl Screen for LevelsScreen {
    fn handle_key(&mut self, key: KeyCode, conn: &Connection) -> ScreenAction {
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                ScreenAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.niveles.len().saturating_sub(1);
                self.selected = self.selected.saturating_add(1).min(max);
                ScreenAction::None
            }
            KeyCode::Enter => {
                if self.niveles.is_empty() {
                    return ScreenAction::None;
                }
                let (nivel, _) = &self.niveles[self.selected];
                let linhas = if let Some(by_level) = &self.lines_by_level {
                    by_level.get(nivel).cloned().unwrap_or_default()
                } else {
                    carregar_linhas(conn, &self.nome_do_container, nivel)
                };
                ScreenAction::Push(Box::new(LinesScreen::new(
                    self.nome_do_container.clone(),
                    nivel.clone(),
                    linhas,
                )))
            }
            KeyCode::Esc | KeyCode::Backspace => ScreenAction::Pop,
            _ => ScreenAction::None,
        }
    }

    fn handle_click(&mut self, row: u16, _col: u16, conn: &Connection) -> ScreenAction {
        let Ok((_, h)) = crossterm::terminal::size() else {
            return ScreenAction::None;
        };
        if row >= 1 && row + 1 < h {
            let index = (row as usize).saturating_sub(1) + self.offset;
            if index < self.niveles.len() {
                self.selected = index;
                return self.handle_key(KeyCode::Enter, conn);
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame) {
        let items: Vec<ListItem> = self
            .niveles
            .iter()
            .map(|(nivel, total)| {
                let item = format!("  {:10} {}", nivel, total);
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
            })
            .collect();

        let total: i64 = self.niveles.iter().map(|(_, v)| v).sum();
        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} ", self.nome_do_container)),
            )
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );

        let area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(f.area());

        let visiveis = (area[0].height as usize).saturating_sub(2);
        self.offset = self
            .offset
            .min(self.niveles.len().saturating_sub(visiveis.max(1)));
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

        let help = Paragraph::new(format!(
            "  ↑/↓ navegar  Enter:ver linhas  Esc:voltar  ({} linhas)",
            total
        ))
        .style(Style::default().fg(Color::DarkGray));
        f.render_widget(help, area[1]);
    }
}
