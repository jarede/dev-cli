// Tela de seleção de tipo de app dentro de um container.
// Apresenta os tipos de app detectados (Uvicorn, Elefante, etc.)
// ordenados do maior volume de linhas para o menor.
//
// Navegação: ↑/↓ seleciona, Enter vai para os níveis da app escolhida,
// Esc volta para a lista de containers.

use std::collections::BTreeMap;

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use rusqlite::Connection;

use nucleo::core::{analisar_apps, parse_loguru_line, AppType};
use crate::screens::levels::LevelsScreen;
use crate::screens::loguru_stats::LoguruStatsScreen;
use crate::screens::{Screen, ScreenAction};

pub(crate) struct AppTypeScreen {
    nome_do_container: String,
    apps: Vec<(AppType, BTreeMap<String, Vec<String>>)>,
    selected: usize,
    offset: usize,
}

impl AppTypeScreen {
    pub(crate) fn new(container: String, linhas: Vec<String>) -> Self {
        let analise = analisar_apps(&linhas);
        let mut apps: Vec<_> = analise.into_iter().collect();
        apps.sort_by_key(|(_, grupos)| {
            let total: usize = grupos.values().map(|v| v.len()).sum();
            std::cmp::Reverse(total)
        });
        Self {
            nome_do_container: container,
            apps,
            selected: 0,
            offset: 0,
        }
    }
}

impl Screen for AppTypeScreen {
    fn handle_key(&mut self, key: KeyCode, _conn: &Connection) -> ScreenAction {
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                ScreenAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.apps.len().saturating_sub(1);
                self.selected = self.selected.saturating_add(1).min(max);
                ScreenAction::None
            }
            KeyCode::Enter => {
                if self.apps.is_empty() {
                    return ScreenAction::None;
                }
                let (app, lines_by_level) = &self.apps[self.selected];
                let app_name = app.to_string();
                let titulo = format!("{} / {}", self.nome_do_container, app_name);

                if *app == AppType::Elefante {
                    // Vai direto para estatísticas agregadas (ignora níveis)
                    let todas: Vec<String> = lines_by_level
                        .values()
                        .flat_map(|v| v.iter().cloned())
                        .collect();
                    // Só vai para LoguruStatsScreen se houver linhas parseáveis
                    // como request HTTP; senão cai no LevelsScreen padrão com
                    // contagens por nível (mensagens livres, sem estrutura).
                    if todas.iter().any(|l| parse_loguru_line(l).is_some()) {
                        return ScreenAction::Push(Box::new(LoguruStatsScreen::new(titulo, todas)));
                    }
                }

                let mut niveles: Vec<_> = lines_by_level
                    .iter()
                    .map(|(k, v)| (k.clone(), v.len() as i64))
                    .collect();
                niveles.sort_by_key(|b| std::cmp::Reverse(b.1));
                ScreenAction::Push(Box::new(LevelsScreen::new(
                    titulo,
                    niveles,
                    Some(lines_by_level.clone()),
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
            if index < self.apps.len() {
                self.selected = index;
                return self.handle_key(KeyCode::Enter, conn);
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame) {
        let items: Vec<ListItem> = self
            .apps
            .iter()
            .map(|(app, grupos)| {
                let total: usize = grupos.values().map(|v| v.len()).sum();
                let detalhes: String = {
                    let mut v: Vec<_> = grupos
                        .iter()
                        .map(|(nivel, linhas)| format!("{} {}", nivel, linhas.len()))
                        .collect();
                    v.sort();
                    v.join("  ")
                };
                ListItem::new(format!(
                    "  {:12}  {}  ({})",
                    app.to_string(),
                    detalhes,
                    total
                ))
            })
            .collect();

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
            .min(self.apps.len().saturating_sub(visiveis.max(1)));
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

        let total_apps: usize = self
            .apps
            .iter()
            .map(|(_, g)| g.values().map(|v| v.len()).sum::<usize>())
            .sum();
        let help = Paragraph::new(format!(
            "  ↑/↓ navegar  Enter:ver níveis  Esc:voltar  ({} apps, {} linhas)",
            self.apps.len(),
            total_apps
        ))
        .style(Style::default().fg(Color::DarkGray));
        f.render_widget(help, area[1]);
    }
}
