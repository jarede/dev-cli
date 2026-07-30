// Preview: tela IA · custos — KPIs, heatmap do mês, horas por semana e
// ranking por modelo. Stub de /api/ia/custos + /api/ia/cambio com os
// valores plausíveis do handoff (custo US$ 186,40, 412.6M tokens, 86% cache).
import { IaCustos } from 'web'

const INTENSIDADES = [
  2, 3, 1, 0, 0, 4, 5, 3, 2, 1, 0, 0, 2, 3, 4, 2, 1, 0, 0, 3, 2, 4, 3, 1, 0, 0, 2, 3, 4,
]

const CUSTOS = {
  mes: '2026-07',
  disponivel: true,
  tokens: 412_600_000,
  custo_usd: 186.4,
  cache_pct: 86,
  streak_dias: 14,
  melhor_streak_dias: 23,
  heatmap: INTENSIDADES.map((intensidade, i) => ({ dia: i + 1, intensidade })),
  // 1º de julho de 2026 caiu numa quarta (0 = segunda … 6 = domingo).
  offset_semana_dia1: 2,
  modelos: [
    { modelo: 'claude-sonnet-5', provedor: 'anthropic', sessoes: 84, tokens: 268_100_000, custo_usd: 96.2 },
    { modelo: 'claude-opus-5', provedor: 'anthropic', sessoes: 12, tokens: 61_400_000, custo_usd: 71.9 },
    { modelo: 'claude-haiku-4-5', provedor: 'anthropic', sessoes: 31, tokens: 74_300_000, custo_usd: 12.1 },
    { modelo: 'minimax-m2', provedor: 'openrouter', sessoes: 9, tokens: 8_800_000, custo_usd: 6.2 },
  ],
  claude_disponivel: true,
  claude_horas_mes: 61.33,
  claude_media_horas_dia_ativo: 3.2,
  claude_horas_por_semana: [
    { rotulo: '29/06', horas: 14.5 },
    { rotulo: '06/07', horas: 18.3 },
    { rotulo: '13/07', horas: 11.2 },
    { rotulo: '20/07', horas: 12.8 },
    { rotulo: '27/07', horas: 4.5 },
  ],
}

const fetchOriginal = window.fetch.bind(window)
window.fetch = (async (recurso: RequestInfo | URL, init?: RequestInit) => {
  const url = String(recurso)
  if (url.includes('/api/ia/custos')) {
    return new Response(JSON.stringify(CUSTOS), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }
  if (url.includes('/api/ia/cambio')) {
    return new Response(JSON.stringify({ usd_brl: 5.42 }), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }
  return fetchOriginal(recurso, init)
}) as typeof fetch

export const MesAtual = () => <IaCustos />
