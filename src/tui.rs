// Módulo do TUI (Terminal User Interface) para navegar nas estatísticas de
// logs coletadas no SQLite.
//
// ORQUESTRAÇÃO: este arquivo só gerencia o terminal (raw mode, tela
// alternada) e a pilha de telas; a lógica de cada tela (navegação, desenho)
// vive em `screens/`. O loop principal é: desenha a tela do topo da pilha,
// espera uma tecla, delega para `handle_key` da tela atual, e age conforme
// a `ScreenAction` devolvida.
//
// docs: https://docs.rs/ratatui/latest/ratatui/
// docs: https://docs.rs/crossterm/latest/crossterm/

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use rusqlite::Connection;

use crate::screens::containers::ContainerScreen;
use crate::screens::{Screen, ScreenAction};

/// Ponto de entrada da TUI: prepara o terminal, mantém uma pilha de telas
/// (cada uma com seu próprio estado e lógica) e restaura o terminal ao sair.
pub(crate) fn run_tui(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let mut terminal = ratatui::Terminal::new(backend)?;

    // Pilha de telas: começa na lista de containers. Cada Enter empilha
    // uma tela-filha (níveis → linhas); Esc/Backspace desempilha.
    let mut screens: Vec<Box<dyn Screen>> = vec![Box::new(ContainerScreen::new(conn)?)];

    let res = loop {
        terminal.draw(|f| {
            if let Some(screen) = screens.last_mut() {
                screen.draw(f);
            }
        })?;

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
