// Preview: tela Histórico — strips de 24 células por container. O
// componente busca /api/containers/historico; o stub devolve intensidades
// plausíveis (pico de erros no prezzo no meio da tarde).
import { Historico } from 'web'

const agora = Math.floor(Date.now() / 1000)
const inicioHora = agora - (agora % 3600)

// 24 células (mais antiga primeiro) a partir de um perfil de intensidades.
function strip(perfil: number[], escala: number) {
  return perfil.map((intensidade, i) => ({
    hora: inicioHora - (23 - i) * 3600,
    quantidade: intensidade === 0 ? 0 : intensidade * escala,
    intensidade,
  }))
}

const HISTORICO = [
  { nome: 'prezzo', horas: strip([0, 0, 1, 1, 2, 1, 0, 1, 2, 3, 4, 5, 4, 3, 2, 2, 1, 1, 2, 1, 1, 0, 1, 2], 12), total: 486 },
  { nome: 'ecomm', horas: strip([0, 1, 0, 0, 1, 1, 0, 0, 1, 2, 2, 1, 1, 2, 3, 1, 0, 1, 0, 0, 1, 1, 0, 1], 4), total: 84 },
  { nome: 'supply', horas: strip([1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], 2), total: 4 },
  { nome: 'bapi', horas: strip([0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0], 1), total: 2 },
  { nome: 'intranet', horas: strip([0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0], 1), total: 4 },
]

const fetchOriginal = window.fetch.bind(window)
window.fetch = (async (recurso: RequestInfo | URL, init?: RequestInit) => {
  if (String(recurso).includes('/api/containers/historico')) {
    return new Response(JSON.stringify(HISTORICO), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }
  return fetchOriginal(recurso, init)
}) as typeof fetch

export const UltimasVinteQuatroHoras = () => <Historico />
