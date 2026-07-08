// Raiz da lib `nucleo`: o coração do dev-cli, sem NENHUMA dependência de
// terminal (clap/ratatui/cores). A regra de arquitetura do projeto:
//   - NÚCLEO PURO (core, metricas): funções texto/valores -> valores,
//     100% testáveis com strings inline;
//   - CASCA DE IO (db, executor, coletor): SQLite, processos docker/ssh;
//   - APRESENTAÇÃO fica nos binários (crates/cli renderiza; o futuro
//     crates/servidor serializa JSON).
// Num workspace, `pub mod` aqui é o que torna cada módulo visível para os
// OUTROS crates (diferente de `pub(crate)`, que restringe ao próprio crate).
// docs: https://doc.rust-lang.org/reference/visibility-and-privacy.html
pub mod coletor;
pub mod config;
pub mod core;
pub mod db;
pub mod executor;
pub mod metricas;
