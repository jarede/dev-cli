// Módulo `panorama`: coleta de inventário de infraestrutura — containers
// Docker de N hosts, volume por vhost lido do log de um proxy reverso,
// repositórios de uma instância GitLab e saúde dos hosts — gravado como um
// snapshot JSON em disco para consumo por uma aplicação externa.
//
// Arquitetura dentro do módulo (espelhando a regra do workspace):
//   - `snapshot.rs`   — o CONTRATO: o formato do snapshot (structs serde).
//                       Nada de coleta nem I/O aqui.
//   - módulos PURA (sem I/O, testáveis com strings inline): `segredos.rs`,
//     `identidade.rs`, `proxy.rs`.
//   - módulos de IO (recebem um `Executor`/config): `docker.rs`, `sistema.rs`;
//     `gitlab.rs` (API REST) já está implementado nesta branch.
//
// Cada novo módulo é adicionado aqui na sua própria issue. `ResultadoColeta`
// mora neste arquivo porque é a linguagem comum entre os coletores e o
// orquestrador — não faz parte do contrato serializado, mas todos dependem.

use crate::panorama::snapshot::ErroColeta;

pub mod docker;
pub mod gitlab;
pub mod identidade;
pub mod proxy;
pub mod segredos;
pub mod sistema;
pub mod snapshot;

/// Resultado de uma coleta.
///
/// A separação `dados`/`erros` é o que permite falha parcial ser o
/// comportamento CORRETO: `dados: Some(parciais)` com `erros` não-vazio
/// significa "consegui parte"; `dados: None` significa "não consegui". O
/// consumidor deve mostrar "dados indisponíveis" nesse caso — jamais "zero",
/// porque zero e "não sei" são coisas diferentes e nunca devem se confundir
/// na tela.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResultadoColeta<T> {
    pub dados: Option<T>,
    pub erros: Vec<ErroColeta>,
}

impl<T> ResultadoColeta<T> {
    /// Dados parciais acompanhados de erros de alguns componentes.
    pub fn parcial(dados: T, erros: Vec<ErroColeta>) -> Self {
        Self {
            dados: Some(dados),
            erros,
        }
    }

    /// Nada foi coletado; resta apenas a descrição da falha.
    pub fn falha(mensagem: impl Into<String>, coletor: impl Into<String>) -> Self {
        Self {
            dados: None,
            erros: vec![ErroColeta {
                coletor: coletor.into(),
                mensagem: mensagem.into(),
            }],
        }
    }
}
