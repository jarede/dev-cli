// Tela final da TUI: linhas de log cruas de um nível específico de um
// container. Linhas longas vêm truncadas por padrão; Enter alterna entre
// expandir e recolher a linha selecionada.
//
// Navegação: ↑/↓ navega, Enter expande/recolhe, Esc volta para os níveis.

use std::collections::HashSet;

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use rusqlite::Connection;

use nucleo::core::{format_loguru_entry, parse_loguru_line};
use crate::screens::{Screen, ScreenAction};

pub(crate) struct LinesScreen {
    nome_do_container: String,
    nome_do_nivel: String,
    linhas: Vec<String>,
    selected: usize,
    offset: usize,
    expanded: HashSet<usize>,
}

impl LinesScreen {
    pub(crate) fn new(container: String, nivel: String, linhas: Vec<String>) -> Self {
        // Tenta parsear cada linha como Loguru/Elefante; se conseguir,
        // substitui pela versão formatada com campos extraídos.
        let linhas: Vec<String> = linhas
            .iter()
            .map(|l| {
                if let Some(e) = parse_loguru_line(l) {
                    format_loguru_entry(&e)
                } else {
                    l.clone()
                }
            })
            .collect();

        Self {
            nome_do_container: container,
            nome_do_nivel: nivel,
            linhas,
            selected: 0,
            offset: 0,
            expanded: HashSet::new(),
        }
    }
}

impl Screen for LinesScreen {
    fn handle_key(&mut self, key: KeyCode, _conn: &Connection) -> ScreenAction {
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                ScreenAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.linhas.len().saturating_sub(1);
                self.selected = self.selected.saturating_add(1).min(max);
                ScreenAction::None
            }
            KeyCode::Enter if !self.linhas.is_empty() => {
                if self.expanded.contains(&self.selected) {
                    self.expanded.remove(&self.selected);
                } else {
                    self.expanded.insert(self.selected);
                }
                ScreenAction::None
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
            if index < self.linhas.len() {
                self.selected = index;
                return self.handle_key(KeyCode::Enter, conn);
            }
        }
        ScreenAction::None
    }

    fn draw(&mut self, f: &mut Frame) {
        let area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(1)])
            .split(f.area());

        // Largura disponível para o texto dentro da lista (desconta bordas)
        let max_width = (area[0].width as usize).saturating_sub(2).max(1);

        // Monta os itens da lista. Linhas expandidas quebram em múltiplas
        // linhas visuais para caber na largura da tela; linhas colapsadas
        // vêm truncadas com "…".
        let items: Vec<ListItem> = self
            .linhas
            .iter()
            .enumerate()
            .map(|(i, linha)| {
                if self.expanded.contains(&i) {
                    let sub = wrap_line(linha, max_width);
                    let text = Text::from(sub.into_iter().map(Line::from).collect::<Vec<Line>>());
                    ListItem::new(text)
                } else {
                    ListItem::new(truncar(linha, max_width))
                        .style(Style::default().fg(Color::DarkGray))
                }
            })
            .collect();

        let titulo = if self.nome_do_nivel.is_empty() {
            format!(" {} ", self.nome_do_container)
        } else {
            format!(" {} / {} ", self.nome_do_container, self.nome_do_nivel)
        };
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(titulo))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );

        let visiveis = (area[0].height as usize).saturating_sub(2);
        self.offset = self
            .offset
            .min(self.linhas.len().saturating_sub(visiveis.max(1)));
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

        let total = self.linhas.len();
        let help = Paragraph::new(format!(
            "  ↑/↓ navegar  Enter:expandir/recolher  Esc:voltar  ({} linhas)",
            total
        ))
        .style(Style::default().fg(Color::DarkGray));
        f.render_widget(help, area[1]);
    }
}

// --- Utilitários -----------------------------------------------------------

/// Carrega TODAS as linhas de log de um container (sem filtrar por nível),
/// na ordem em que foram inseridas. Usado pela detecção de app types.
///
/// Devolve `Vec` "vazio silencioso" em caso de erro de SQL em vez de
/// propagar um `Result`: esta função é chamada de dentro de
/// `Screen::handle_key` (ver `dashboard.rs`), cuja assinatura de trait
/// devolve `ScreenAction`, não `Result` — mudar isso propagaria por toda a
/// árvore de telas. Como o SQL é fixo e o schema é garantido por
/// `init_db`, o erro esperado aqui é só "banco fechado/corrompido", que a UI
/// prefere degradar como "sem linhas" a entrar em pânico (mesmo padrão de
/// `filter_map(|r| r.ok())` usado em `nucleo::db`).
pub(crate) fn carregar_todas_linhas(conn: &Connection, container: &str) -> Vec<String> {
    let Ok(mut stmt) =
        conn.prepare("SELECT line FROM log_lines WHERE container_name = ?1 ORDER BY id")
    else {
        return Vec::new();
    };
    let Ok(linhas) = stmt.query_map(rusqlite::params![container], |r| r.get(0)) else {
        return Vec::new();
    };
    linhas.filter_map(|r| r.ok()).collect()
}

/// Carrega linhas de log do banco para um container e nível específicos.
/// Mesma justificativa de `carregar_todas_linhas` para não usar `unwrap()`.
pub(crate) fn carregar_linhas(conn: &Connection, container: &str, nivel: &str) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT line FROM log_lines
         WHERE container_name = ?1 AND level = ?2
         ORDER BY id",
    ) else {
        return Vec::new();
    };
    let Ok(linhas) = stmt.query_map(rusqlite::params![container, nivel], |r| r.get(0)) else {
        return Vec::new();
    };
    linhas.filter_map(|r| r.ok()).collect()
}

/// Corta a string em `max` bytes e acrescenta "…" para indicar que há mais
/// texto.
fn truncar(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

/// Quebra uma string longa em linhas de no máximo `max_width` caracteres.
/// Usa `chars()` para operar sobre caracteres Unicode em vez de bytes,
/// evitando panic em caracteres multi-byte (emoji, acentos, CJK).
fn wrap_line(s: &str, max_width: usize) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    chars
        .chunks(max_width)
        .map(|c| c.iter().collect::<String>())
        .collect()
}
