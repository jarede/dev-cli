// Preview: nav sticky do portal (marca + rotas + resumo global derivado).
// O Router vem do provider global (RoteadorPreview / MemoryRouter).
import { Cabecalho } from 'web'

const agora = Math.floor(Date.now() / 1000)

const base = {
  status: 'running',
  uptime: 'Up 2 days',
  crits: 0,
  c5xx: 0,
  c4xx: 0,
  p95_seg: 0.2,
  max_seg: 1.0,
  total_linhas: 1000,
  ultima_coleta: agora - 12,
}

const CONTAINERS = [
  { ...base, nome: 'prezzo', erros: 231, crits: 4, reqs: 18234, severidade: 'Vermelho' as const },
  { ...base, nome: 'ecomm', erros: 38, reqs: 9412, severidade: 'Amarelo' as const },
  { ...base, nome: 'supply', status: 'stopped', uptime: '', erros: 0, reqs: 0, severidade: 'Parado' as const },
  { ...base, nome: 'bapi', erros: 2, reqs: 6120, severidade: 'Verde' as const },
  { ...base, nome: 'intranet', erros: 6, reqs: 3105, severidade: 'Verde' as const },
]

export const ComProblemas = () => <Cabecalho containers={CONTAINERS} erro={null} />

export const ApiFora = () => (
  <Cabecalho
    containers={CONTAINERS}
    erro="API respondeu 502 em /api/containers"
  />
)
