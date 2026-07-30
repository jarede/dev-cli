// Preview: o drawer de drill-down. O componente busca as linhas via
// /api/containers/{nome}/linhas — o stub de fetch abaixo devolve linhas
// realistas (estilo Loguru/Oracle) para o render ser o verdadeiro.
import { PainelContainer } from 'web'

const LINHAS = [
  { nivel: 'CRITICAL', linha: 'ORA-00060: deadlock detected while waiting for resource | recalculo_precos worker=3 sku=88412', collected_at: 0 },
  { nivel: 'ERROR', linha: '2026-07-29 14:02:11.404 | ERROR | prezzo.oracle:executar:88 - ORA-01555: snapshot too old: rollback segment number 9', collected_at: 0 },
  { nivel: 'WARNING', linha: '2026-07-29 14:01:58.021 | WARNING | prezzo.filas:consumir:41 - fila recalculo com 1.2k itens pendentes', collected_at: 0 },
  { nivel: 'INFO', linha: '2026-07-29 14:01:44.310 | INFO | prezzo.api:precos:12 - GET /precos/88412 200 (18ms)', collected_at: 0 },
  { nivel: 'INFO', linha: '2026-07-29 14:01:31.007 | INFO | prezzo.api:precos:12 - GET /precos/91230 200 (14ms)', collected_at: 0 },
  { nivel: 'DEBUG', linha: '2026-07-29 14:01:30.900 | DEBUG | prezzo.cache:ler:7 - hit sku=91230 ttl=41s', collected_at: 0 },
]

const fetchOriginal = window.fetch.bind(window)
window.fetch = (async (recurso: RequestInfo | URL, init?: RequestInit) => {
  const url = String(recurso)
  if (url.includes('/api/containers/') && url.includes('/linhas')) {
    const nivel = new URL(url, 'http://x').searchParams.get('nivel')
    const corpo = nivel ? LINHAS.filter((l) => l.nivel === nivel) : LINHAS
    return new Response(JSON.stringify(corpo), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }
  return fetchOriginal(recurso, init)
}) as typeof fetch

export const TodosOsNiveis = () => (
  <PainelContainer nome="prezzo" aoFechar={() => {}} />
)

export const FiltradoEmError = () => (
  <PainelContainer nome="prezzo" aoFechar={() => {}} nivelInicial="ERROR" />
)
