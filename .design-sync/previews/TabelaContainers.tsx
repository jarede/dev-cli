// Preview: tabela de containers da Visão geral — "piores primeiros", com os
// containers-exemplo do handoff (prezzo vermelho, ecomm amarelo, supply
// parado, bapi/intranet verdes). err > 50 e crit > 0 pintam de vermelho.
import { TabelaContainers } from 'web'

const agora = Math.floor(Date.now() / 1000)

const CONTAINERS = [
  {
    nome: 'prezzo',
    status: 'running',
    uptime: 'Up 2 days',
    erros: 231,
    crits: 4,
    c5xx: 87,
    c4xx: 412,
    reqs: 18234,
    p95_seg: 2.41,
    max_seg: 8.03,
    total_linhas: 52310,
    ultima_coleta: agora - 12,
    severidade: 'Vermelho' as const,
  },
  {
    nome: 'ecomm',
    status: 'running',
    uptime: 'Up 6 hours',
    erros: 38,
    crits: 0,
    c5xx: 9,
    c4xx: 133,
    reqs: 9412,
    p95_seg: 0.87,
    max_seg: 3.2,
    total_linhas: 18770,
    ultima_coleta: agora - 12,
    severidade: 'Amarelo' as const,
  },
  {
    nome: 'supply',
    status: 'stopped',
    uptime: '',
    erros: 0,
    crits: 0,
    c5xx: 0,
    c4xx: 0,
    reqs: 0,
    p95_seg: null,
    max_seg: null,
    total_linhas: 0,
    ultima_coleta: agora - 3 * 3600,
    severidade: 'Parado' as const,
  },
  {
    nome: 'bapi',
    status: 'running',
    uptime: 'Up 9 days',
    erros: 2,
    crits: 0,
    c5xx: 0,
    c4xx: 21,
    reqs: 6120,
    p95_seg: 0.14,
    max_seg: 0.9,
    total_linhas: 8340,
    ultima_coleta: agora - 12,
    severidade: 'Verde' as const,
  },
  {
    nome: 'intranet',
    status: 'running',
    uptime: 'Up 12 days',
    erros: 6,
    crits: 0,
    c5xx: 1,
    c4xx: 44,
    reqs: 3105,
    p95_seg: 0.22,
    max_seg: 1.1,
    total_linhas: 4980,
    ultima_coleta: agora - 12,
    severidade: 'Verde' as const,
  },
]

export const PioresPrimeiros = () => (
  <TabelaContainers containers={CONTAINERS} selecionado={null} aoSelecionar={() => {}} />
)

export const LinhaSelecionada = () => (
  <TabelaContainers containers={CONTAINERS} selecionado="prezzo" aoSelecionar={() => {}} />
)

export const Vazia = () => (
  <TabelaContainers containers={[]} selecionado={null} aoSelecionar={() => {}} />
)
