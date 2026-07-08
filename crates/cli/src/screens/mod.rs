// Trait `Screen` e enum `ScreenAction` para o sistema de telas empilháveis.
//
// Cada tela (containers, níveis, linhas) implementa `Screen` e vive numa
// pilha dentro do loop principal (`tui.rs`). `handle_key` processa teclas
// e devolve uma ação (navegar, empilhar, desempilhar, sair); `draw` desenha
// o quadro. O `&Connection` permite carregar dados do banco a qualquer
// momento, sem depender de estado externo.

pub mod app_types;
pub mod containers;
pub mod levels;
pub mod lines;
pub mod loguru_stats;

use ratatui::Frame;
use rusqlite::Connection;

pub(crate) trait Screen {
    /// Processa uma tecla e devolve a ação resultante.
    fn handle_key(&mut self, key: crossterm::event::KeyCode, conn: &Connection) -> ScreenAction;
    /// Processa um clique do mouse na posição (row, col) — coordenadas
    /// zero-based a partir do canto superior esquerdo do terminal.
    /// A implementação padrão ignora o clique.
    fn handle_click(&mut self, _row: u16, _col: u16, _conn: &Connection) -> ScreenAction {
        ScreenAction::None
    }
    /// Desenha o quadro no terminal.
    fn draw(&mut self, f: &mut Frame);
}

/// Ações que uma tela pode devolver após processar uma tecla.
pub(crate) enum ScreenAction {
    /// Empilha uma nova tela (ex.: Enter vai de níveis para linhas).
    Push(Box<dyn Screen>),
    /// Desempilha a tela atual (Esc volta para a tela anterior).
    Pop,
    /// Encerra a TUI.
    Quit,
    /// Nenhuma ação (tecla ignorada).
    None,
}
