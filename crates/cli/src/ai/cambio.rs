// Câmbio USD -> BRL: a busca ao vivo e a conversão pura vivem em
// `nucleo::cambio` (extraído de lá — achado 12 da revisão do redesign do
// portal — para o dev-server reaproveitar exatamente a mesma chamada HTTP
// em vez de manter uma taxa hardcoded). Este arquivo só reexporta, para o
// resto do CLI continuar escrevendo `cambio::buscar_taxa_usd_brl`/
// `cambio::converter_para_brl` sem precisar mudar imports.
pub use nucleo::cambio::{buscar_taxa_usd_brl, converter_para_brl};
