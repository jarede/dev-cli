// Tela inicial da TUI: o dashboard "onde estão os problemas".
//
// Mostra todos os containers ranqueados por severidade (parado > vermelho >
// amarelo > verde), com erros, status HTTP e tempos de resposta da janela
// configurada. A tela NÃO coleta nada: ela lê os agregados do SQLite
// (resumo_janela) e é avisada pela thread coletora via `atualizar`.
//
// Navegação: ↑/↓ seleciona, Enter mergulha no container (drill-down),
// r pede coleta imediata, q sai.

use std::sync::mpsc::Sender;
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState},
};
use rusqlite::Connection;

use nucleo::coletor::{ComandoColetor, EventoColeta};
use nucleo::config::Limiares;
use nucleo::db::resumo_janela;
use nucleo::metricas::{ResumoContainer, Severidade, severidade};

use crate::screens::app_types::AppTypeScreen;
use crate::screens::lines::carregar_todas_linhas;
use crate::screens::{Screen, ScreenAction};

/// Timestamp Unix atual (segundos).
fn agora_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

pub(crate) struct DashboardScreen {
    /// Resumos já classificados e ordenados (pior primeiro).
    itens: Vec<(Severidade, ResumoContainer)>,
    selected: usize,
    limiares: Limiares,
    /// Minutos da janela de estatísticas (só para exibir no título).
    janela_min: u64,
    /// Rótulo da origem dos dados: "local" ou "ssh: user@host".
    origem: String,
    /// Momento (unix) da última coleta bem-sucedida vista pela tela.
    ultima_coleta_ok: Option<i64>,
    /// Última falha de coleta (mensagem, quando) — some no próximo sucesso.
    falha: Option<(String, i64)>,
    /// Canal para pedir "coletar agora" à thread coletora (tecla r).
    comandos: Sender<ComandoColetor>,
}

impl DashboardScreen {
    pub(crate) fn new(
        conn: &Connection,
        limiares: Limiares,
        janela_min: u64,
        origem: String,
        comandos: Sender<ComandoColetor>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut tela = Self {
            itens: Vec::new(),
            selected: 0,
            limiares,
            janela_min,
            origem,
            ultima_coleta_ok: None,
            falha: None,
            comandos,
        };
        tela.recarregar(conn)?;
        Ok(tela)
    }

    /// Relê os agregados da janela no banco, classifica e ordena.
    fn recarregar(&mut self, conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
        let corte = agora_unix() - (self.janela_min as i64) * 60;
        let mut itens: Vec<(Severidade, ResumoContainer)> = resumo_janela(conn, corte)?
            .into_iter()
            .map(|r| (severidade(&r, &self.limiares), r))
            .collect();
        // Pior primeiro: ordena por severidade DESC e, dentro dela, por
        // quantidade de problemas DESC. `sort_by` com `cmp` invertido
        // (b antes de a) é o idioma para ordem decrescente.
        // docs: https://doc.rust-lang.org/std/vec/struct.Vec.html#method.sort_by
        itens.sort_by(|a, b| {
            let problemas_a = a.1.erros + a.1.crits + a.1.c5xx;
            let problemas_b = b.1.erros + b.1.crits + b.1.c5xx;
            (b.0, problemas_b).cmp(&(a.0, problemas_a))
        });
        self.itens = itens;
        self.selected = self.selected.min(self.itens.len().saturating_sub(1));
        Ok(())
    }
}

/// Ícone + cor de cada severidade.
fn aparencia(sev: Severidade) -> (&'static str, Color) {
    match sev {
        Severidade::Parado => ("✖", Color::Red),
        Severidade::Vermelho => ("●", Color::Red),
        Severidade::Amarelo => ("●", Color::Yellow),
        Severidade::Verde => ("○", Color::Green),
    }
}

/// Formata `Option<f64>` de segundos como "1.23s" ou "—".
fn fmt_seg(valor: Option<f64>) -> String {
    match valor {
        Some(v) => format!("{v:.2}s"),
        None => "—".to_string(),
    }
}

/// Formata um inteiro, trocando zero por "—" para aliviar a tabela.
fn fmt_n(valor: i64) -> String {
    if valor == 0 {
        "—".to_string()
    } else {
        valor.to_string()
    }
}

impl Screen for DashboardScreen {
    fn handle_key(&mut self, key: KeyCode, conn: &Connection) -> ScreenAction {
        match key {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
                ScreenAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max = self.itens.len().saturating_sub(1);
                self.selected = self.selected.saturating_add(1).min(max);
                ScreenAction::None
            }
            KeyCode::Enter => {
                if self.itens.is_empty() {
                    return ScreenAction::None;
                }
                let nome = self.itens[self.selected].1.nome.clone();
                let linhas = carregar_todas_linhas(conn, &nome);
                ScreenAction::Push(Box::new(AppTypeScreen::new(nome, linhas)))
            }
            KeyCode::Char('r') => {
                // Pede um ciclo imediato; o resultado chega via `atualizar`.
                let _ = self.comandos.send(ComandoColetor::ColetarAgora);
                ScreenAction::None
            }
            KeyCode::Char('q') | KeyCode::Esc => ScreenAction::Quit,
            _ => ScreenAction::None,
        }
    }

    fn atualizar(&mut self, evento: &EventoColeta, conn: &Connection) {
        match evento {
            EventoColeta::Novo => {
                self.ultima_coleta_ok = Some(agora_unix());
                self.falha = None;
                // Erro ao reler é tratado como falha "de coleta" na UI —
                // melhor mostrar o problema do que derrubar a TUI.
                if let Err(erro) = self.recarregar(conn) {
                    self.falha = Some((erro.to_string(), agora_unix()));
                }
            }
            EventoColeta::Falha(mensagem) => {
                self.falha = Some((mensagem.clone(), agora_unix()));
            }
        }
    }

    fn draw(&mut self, f: &mut Frame) {
        let area = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // cabeçalho (origem + resumo global)
                Constraint::Min(1),    // tabela
                Constraint::Length(1), // rodapé de teclas
            ])
            .split(f.area());

        // --- Cabeçalho -------------------------------------------------
        let agora = agora_unix();
        let coleta = match (self.ultima_coleta_ok, &self.falha) {
            (_, Some((mensagem, quando))) => format!(
                "⚠ coleta falhou há {}s: {}",
                agora - quando,
                mensagem.lines().next().unwrap_or("")
            ),
            (Some(quando), None) => format!("coleta há {}s", agora - quando),
            (None, None) => "aguardando primeira coleta…".to_string(),
        };
        let problemas = self
            .itens
            .iter()
            .filter(|(sev, _)| *sev >= Severidade::Vermelho)
            .count();
        let total_reqs: i64 = self.itens.iter().map(|(_, r)| r.reqs).sum();
        let total_erros: i64 = self.itens.iter().map(|(_, r)| r.erros + r.crits).sum();
        let cabecalho = format!(
            " dev-cli · {} · {} · janela {}min\n ▍{} problema(s) · {} containers · {} reqs · {} erros",
            self.origem,
            coleta,
            self.janela_min,
            problemas,
            self.itens.len(),
            total_reqs,
            total_erros,
        );
        let estilo_cabecalho = if self.falha.is_some() {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Cyan)
        };
        f.render_widget(Paragraph::new(cabecalho).style(estilo_cabecalho), area[0]);

        // --- Tabela -----------------------------------------------------
        let linhas: Vec<Row> = self
            .itens
            .iter()
            .map(|(sev, r)| {
                let (icone, cor) = aparencia(*sev);
                Row::new(vec![
                    Cell::from(icone).style(Style::default().fg(cor)),
                    Cell::from(r.nome.clone()),
                    Cell::from(r.uptime.clone()),
                    Cell::from(fmt_n(r.erros)),
                    Cell::from(fmt_n(r.crits)),
                    Cell::from(fmt_n(r.c5xx)),
                    Cell::from(fmt_n(r.c4xx)),
                    Cell::from(fmt_seg(r.p95_seg)),
                    Cell::from(fmt_seg(r.max_seg)),
                    Cell::from(fmt_n(r.reqs)),
                ])
                .style(Style::default().fg(cor))
            })
            .collect();

        let tabela = Table::new(
            linhas,
            [
                Constraint::Length(2),  // ícone
                Constraint::Min(20),    // container
                Constraint::Length(16), // uptime
                Constraint::Length(5),  // ERR
                Constraint::Length(5),  // CRIT
                Constraint::Length(5),  // 5xx
                Constraint::Length(5),  // 4xx
                Constraint::Length(8),  // p95
                Constraint::Length(8),  // máx
                Constraint::Length(6),  // reqs
            ],
        )
        .header(
            Row::new(vec![
                "",
                "CONTAINER",
                "STATUS",
                "ERR",
                "CRIT",
                "5xx",
                "4xx",
                "p95",
                "máx",
                "reqs",
            ])
            .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(Block::default().borders(Borders::ALL).title(" Containers "))
        .row_highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        // `TableState` guarda a seleção; o ratatui rola a tabela sozinho
        // para manter a linha selecionada visível.
        // docs: https://docs.rs/ratatui/latest/ratatui/widgets/struct.TableState.html
        let mut estado = TableState::default();
        estado.select(Some(self.selected));
        f.render_stateful_widget(tabela, area[1], &mut estado);

        // --- Rodapé -----------------------------------------------------
        let ajuda = Paragraph::new("  ↑/↓ navegar · Enter detalhes · r coletar agora · q sair")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(ajuda, area[2]);
    }
}
