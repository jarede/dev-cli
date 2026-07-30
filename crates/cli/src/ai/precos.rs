// Mudança de morada: o cálculo de preços vive em `nucleo::ia_claude`
// (o dev-server também precisa dele). Este re-export mantém os caminhos
// `crate::ai::precos::*` do CLI funcionando sem mudar nenhum import —
// mesmo padrão do re-export de `horas_sessao` em `render.rs`.
pub use nucleo::ia_claude::calcular_custo_detalhado;
pub use nucleo::ia_claude::distribuir_custo_proporcional;
