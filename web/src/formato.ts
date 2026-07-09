export function formatarSegundos(valor: number | null): string {
  if (valor === null) return '—'
  return `${valor.toFixed(2)}s`
}

export function formatarNumero(valor: number): string {
  if (valor >= 1000) return `${(valor / 1000).toFixed(1)}k`
  return String(valor)
}

export function formatarHaQuanto(tsUnix: number, agoraMs: number = Date.now()): string {
  if (tsUnix <= 0) return 'nunca'
  const seg = Math.max(0, Math.floor(agoraMs / 1000) - tsUnix)
  if (seg < 60) return `há ${seg}s`
  if (seg < 3600) return `há ${Math.floor(seg / 60)}min`
  return `há ${Math.floor(seg / 3600)}h`
}
