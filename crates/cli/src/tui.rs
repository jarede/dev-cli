// Módulo do TUI (Terminal User Interface).
//
// ORQUESTRAÇÃO: este arquivo só gerencia o terminal (raw mode, tela
// alternada) e a pilha de telas; a lógica de cada tela vive em `screens/`.
//
// O loop principal mudou para suportar coleta AO VIVO: em vez do
// `event::read()` bloqueante (que só acorda com tecla), usamos
// `event::poll(250ms)` — a cada 250ms sem tecla o loop dá uma volta,
// drena o canal de eventos da thread coletora e redesenha. Assim o
// dashboard atualiza sozinho e o relógio "coleta há Xs" anda.
//
// docs: https://docs.rs/ratatui/latest/ratatui/
// docs: https://docs.rs/crossterm/latest/crossterm/event/fn.poll.html

use std::sync::mpsc::Receiver;
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use nucleo::coletor::EventoColeta;
use rusqlite::Connection;

use crate::screens::{Screen, ScreenAction};

/// Ponto de entrada da TUI. `tela_inicial` define o que abre primeiro
/// (dashboard ao vivo ou drill-down estático); `eventos` é o canal da
/// thread coletora (None = TUI estática, sem coleta ao vivo).
pub(crate) fn run_tui(
    conn: &Connection,
    tela_inicial: Box<dyn Screen>,
    eventos: Option<Receiver<EventoColeta>>,
) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // Pilha de telas: Enter empilha uma tela-filha; Esc/Backspace desempilha.
    let mut screens: Vec<Box<dyn Screen>> = vec![tela_inicial];

    let res = loop {
        terminal.draw(|f| {
            if let Some(screen) = screens.last_mut() {
                screen.draw(f);
            }
        })?;

        // 1. Entrega à tela do topo os eventos da coleta que chegaram.
        // `try_recv` não bloqueia: drena o que houver e segue.
        // docs: https://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html#method.try_recv
        if let Some(rx) = &eventos {
            while let Ok(evento) = rx.try_recv() {
                if let Some(screen) = screens.last_mut() {
                    screen.atualizar(&evento, conn);
                }
            }
        }

        // 2. Espera tecla/mouse por até 250ms; sem nada, volta ao draw
        // (é isso que faz o dashboard "andar" sem o usuário digitar).
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        let action = match event::read()? {
            Event::Key(key) => screens
                .last_mut()
                .map(|s| s.handle_key(key.code, conn))
                .unwrap_or(ScreenAction::Quit),
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::ScrollDown => screens
                    .last_mut()
                    .map(|s| s.handle_key(KeyCode::Down, conn))
                    .unwrap_or(ScreenAction::None),
                MouseEventKind::ScrollUp => screens
                    .last_mut()
                    .map(|s| s.handle_key(KeyCode::Up, conn))
                    .unwrap_or(ScreenAction::None),
                MouseEventKind::Down(_) => screens
                    .last_mut()
                    .map(|s| s.handle_click(mouse.row, mouse.column, conn))
                    .unwrap_or(ScreenAction::None),
                _ => ScreenAction::None,
            },
            _ => ScreenAction::None,
        };

        match action {
            ScreenAction::Push(s) => screens.push(s),
            ScreenAction::Pop => {
                screens.pop();
                if screens.is_empty() {
                    break Ok(());
                }
            }
            ScreenAction::Quit => break Ok(()),
            ScreenAction::None => {}
        }
    };

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;

    res
}
