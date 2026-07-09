// Funções PURAS de formatação: valor -> string, sem DOM e sem estado —
// o equivalente web do "núcleo puro" do crates/nucleo, 100% testável.

/// Segundos com 2 casas ("1.23s"); null (sem dados) vira travessão.
export function formatarSegundos(valor: number | null): string {
  if (valor === null) return '—'
  return `${valor.toFixed(2)}s`
}

/// Abrevia milhares: 1234 -> "1.2k". Mantém < 1000 como está.
export function formatarNumero(valor: number): string {
  if (valor >= 1000) return `${(valor / 1000).toFixed(1)}k`
  return String(valor)
}

/// Quanto tempo faz desde um timestamp Unix (segundos): "há 12s"/"há 3min"/
/// "há 2h". `agoraMs` é injetável para os testes serem determinísticos
/// (em produção usa Date.now()).
export function formatarHaQuanto(tsUnix: number, agoraMs: number = Date.now()): string {
  if (tsUnix <= 0) return 'nunca'
  const seg = Math.max(0, Math.floor(agoraMs / 1000) - tsUnix)
  if (seg < 60) return `há ${seg}s`
  if (seg < 3600) return `há ${Math.floor(seg / 60)}min`
  return `há ${Math.floor(seg / 3600)}h`
}
