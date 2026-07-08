// Tela de estatísticas agregadas para logs no formato Loguru/Elefante.
// Apresenta médias por endpoint, métodos, códigos HTTP, etc. com a
// possibilidade de entrar num grupo para ver as linhas individuais.
// Tecla `e` filtra para mostrar apenas grupos com ERROR/CRITICAL.
//
// Navegação: ↑/↓ seleciona, Enter vai para as linhas do grupo,
// e toggle erro, Esc volta para a tela de apps.

use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use rusqlite::Connection;

use nucleo::core::{parse_loguru_line, LoguruEntry};
use crate::screens::lines::LinesScreen;
use crate::screens::{Screen, ScreenAction};

/// (metodo, path, status) → lista de (linha_raw, duracao, nivel)
type GrupoRaw = std::collections::BTreeMap<(String, String, u16), Vec<(String, f64, String)>>;

/// Agregação por (método, path, status).
struct GroupStats {
    metodo: String,
    path: String,
    status: u16,
    count: usize,
    total_duration: f64,
    min_duration: f64,
    max_duration: f64,
    tem_erro: bool,
    linhas_erro: usize,
}

pub(crate) struct LoguruStatsScreen {
    nome_do_container: String,
    grupos: Vec<GroupStats>,
    linhas_por_grupo: Vec<Vec<String>>,
    selected: usize,
    offset: usize,
    show_only_errors: bool,
    /// Mapeia índice exibido → índice real em `self.grupos` e `linhas_por_grupo`.
    exibidos: Vec<usize>,
    total_linhas: usize,
    total_duration: f64,
    niveis: Vec<(String, usize)>,
    metodos: Vec<(String, usize)>,
    status_codes: Vec<(u16, usize)>,
}

impl LoguruStatsScreen {
    pub(crate) fn new(container: String, linhas: Vec<String>) -> Self {
        let entradas: Vec<(String, LoguruEntry)> = linhas
            .iter()
            .filter_map(|l| parse_loguru_line(l).map(|e| (l.clone(), e)))
            .collect();

        let mut grupos: GrupoRaw = std::collections::BTreeMap::new();
        let mut total_duration = 0.0_f64;
        for (linha, e) in &entradas {
            let chave = (e.metodo.clone(), e.path.clone(), e.status);
            grupos
                .entry(chave)
                .or_default()
                .push((linha.clone(), e.duracao_seg, e.level.clone()));
            total_duration += e.duracao_seg;
        }

        let mut lista: Vec<GroupStats> = grupos
            .into_iter()
            .map(|((metodo, path, status), items)| {
                let durations: Vec<f64> = items.iter().map(|(_, d, _)| *d).collect();
                let min_d = durations.iter().cloned().fold(f64::MAX, f64::min);
                let max_d = durations.iter().cloned().fold(f64::MIN, f64::max);
                let total_d: f64 = durations.iter().sum();
                let linhas_erro = items
                    .iter()
                    .filter(|(_, _, lvl)| {
                        let u = lvl.to_uppercase();
                        u == "ERROR" || u == "CRITICAL" || u == "FATAL"
                    })
                    .count();
                GroupStats {
                    metodo,
                    path,
                    status,
                    count: items.len(),
                    total_duration: total_d,
                    min_duration: min_d,
                    max_duration: max_d,
                    tem_erro: linhas_erro > 0,
                    linhas_erro,
                }
            })
            .collect();
        lista.sort_by(|a, b| a.path.cmp(&b.path).then(a.metodo.cmp(&b.metodo)));

        let linhas_por_grupo: Vec<Vec<String>> = lista
            .iter()
            .map(|g| {
                let chave = (g.metodo.clone(), g.path.clone(), g.status);
                entradas
                    .iter()
                    .filter(|(_, e)| {
                        e.metodo == chave.0 && e.path == chave.1 && e.status == chave.2
                    })
                    .map(|(l, _)| l.clone())
                    .collect()
            })
            .collect();

        let mut nivel_count: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut metodo_count: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut status_count: std::collections::BTreeMap<u16, usize> =
            std::collections::BTreeMap::new();
        for (_, e) in &entradas {
            *nivel_count.entry(e.level.clone()).or_default() += 1;
            *metodo_count.entry(e.metodo.clone()).or_default() += 1;
            *status_count.entry(e.status).or_default() += 1;
        }
        let mut niveis: Vec<_> = nivel_count.into_iter().collect();
        niveis.sort();
        let mut metodos: Vec<_> = metodo_count.into_iter().collect();
        metodos.sort();
        let mut status_codes: Vec<_> = status_count.into_iter().collect();
        status_codes.sort();

        let exibidos: Vec<usize> = (0..lista.len()).collect();

        Self {
            nome_do_container: container,
            grupos: lista,
            linhas_por_grupo,
            selected: 0,
            offset: 0,
            show_only_errors: false,
            exibidos,
            total_linhas: entradas.len(),
            total_duration,
            niveis,
            metodos,
            status_codes,
        }
    }

    fn reconstruir_exibidos(&mut self) {
        if self.show_only_errors {
            self.exibidos = self
                .grupos
                .iter()
                .enumerate()
                .filter(|(_, g)| g.tem_erro)
                .map(|(i, _)| i)
                .collect();
        } else {
            self.exibidos = (0..self.grupos.len()).collect();
        }
        self.selected = self.selected.min(self.exibidos.len().saturating_sub(1));
        self.offset = self.offset.min(self.exibidos.len().saturating_sub(1));
    }
}

impl Screen for LoguruStatsScreen {
    fn handle_key(&mut self, key: KeyCode, _conn: &Connection) -> ScreenAction {
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                ScreenAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.exibidos.len().saturating_sub(1);
                self.selected = self.selected.saturating_add(1).min(max);
                ScreenAction::None
            }
            KeyCode::Enter => {
                if self.exibidos.is_empty() || self.selected >= self.exibidos.len() {
                    return ScreenAction::None;
                }
                let real = self.exibidos[self.selected];
                let grupo = &self.grupos[real];
                let linhas = if self.show_only_errors {
                    // Só linhas com nível ERROR/CRITICAL/FATAL deste grupo
                    self.linhas_por_grupo[real]
                        .iter()
                        .filter(|l| {
                            parse_loguru_line(l).is_some_and(|e| {
                                let u = e.level.to_uppercase();
                                u == "ERROR" || u == "CRITICAL" || u == "FATAL"
                            })
                        })
                        .cloned()
                        .collect()
                } else {
                    self.linhas_por_grupo[real].clone()
                };
                ScreenAction::Push(Box::new(LinesScreen::new(
                    format!(
                        "{} / {} {} {}",
                        self.nome_do_container, grupo.metodo, grupo.path, grupo.status
                    ),
                    String::new(),
                    linhas,
                )))
            }
            KeyCode::Char('e') | KeyCode::Char('E') => {
                self.show_only_errors = !self.show_only_errors;
                self.reconstruir_exibidos();
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
            if index < self.exibidos.len() {
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

        let items: Vec<ListItem> = self
            .exibidos
            .iter()
            .map(|&real| {
                let g = &self.grupos[real];
                let avg = if g.count > 0 {
                    g.total_duration / g.count as f64
                } else {
                    0.0
                };
                let estilo = if g.tem_erro {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };
                let erro_tag = if g.tem_erro {
                    format!(" [{} err]", g.linhas_erro)
                } else {
                    String::new()
                };
                ListItem::new(format!(
                    "  {:6} {:20} {:3}  {:4}x  avg {:.4}s  min {:.4}s  max {:.4}s{}",
                    g.metodo,
                    g.path,
                    g.status,
                    g.count,
                    avg,
                    g.min_duration,
                    g.max_duration,
                    erro_tag,
                ))
                .style(estilo)
            })
            .collect();

        let avg_total = if self.total_linhas > 0 {
            self.total_duration / self.total_linhas as f64
        } else {
            0.0
        };
        let niveis_str: String = self
            .niveis
            .iter()
            .map(|(n, c)| format!("{} {}", n, c))
            .collect::<Vec<_>>()
            .join("  ");
        let metodos_str: String = self
            .metodos
            .iter()
            .map(|(m, c)| format!("{} {}", m, c))
            .collect::<Vec<_>>()
            .join("  ");
        let status_str: String = self
            .status_codes
            .iter()
            .map(|(s, c)| format!("{} {}", s, c))
            .collect::<Vec<_>>()
            .join("  ");

        let erro_sufixo = if self.show_only_errors {
            " [SÓ ERROS]"
        } else {
            ""
        };
        let header = format!(
            "{} | {} req, avg {:.4}s | {} | {} | {}{}",
            self.nome_do_container,
            self.total_linhas,
            avg_total,
            niveis_str,
            metodos_str,
            status_str,
            erro_sufixo,
        );

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(header))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            );

        let visiveis = (area[0].height as usize).saturating_sub(2);
        let max_offset = self.exibidos.len().saturating_sub(visiveis.max(1));
        self.offset = self.offset.min(max_offset);
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

        let total = self.total_linhas;
        let sufixo_ajuda = if self.show_only_errors {
            " (só erros)"
        } else {
            ""
        };
        let help = Paragraph::new(format!(
            "  ↑/↓ navegar  Enter:ver linhas  e:filtrar erros  Esc:voltar  ({} grupos, {} linhas{})",
            self.exibidos.len(),
            total,
            sufixo_ajuda,
        ))
        .style(Style::default().fg(Color::DarkGray));
        f.render_widget(help, area[1]);
    }
}
