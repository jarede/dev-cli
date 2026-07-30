// Preview: a tela Visão geral inteira — grid 1fr/380px com tabela à
// esquerda e alertas + feed à direita (composição real dos 3 componentes).
import { VisaoGeral } from 'web'

const agora = Math.floor(Date.now() / 1000)

const base = {
  status: 'running',
  c5xx: 0,
  c4xx: 0,
  total_linhas: 1000,
  ultima_coleta: agora - 12,
}

const CONTAINERS = [
  { ...base, nome: 'prezzo', uptime: 'Up 2 days', erros: 231, crits: 4, c5xx: 87, c4xx: 412, reqs: 18234, p95_seg: 2.41, max_seg: 8.03, severidade: 'Vermelho' as const },
  { ...base, nome: 'ecomm', uptime: 'Up 6 hours', erros: 38, crits: 0, c5xx: 9, c4xx: 133, reqs: 9412, p95_seg: 0.87, max_seg: 3.2, severidade: 'Amarelo' as const },
  { ...base, nome: 'supply', status: 'stopped', uptime: '', erros: 0, crits: 0, reqs: 0, p95_seg: null, max_seg: null, severidade: 'Parado' as const },
  { ...base, nome: 'bapi', uptime: 'Up 9 days', erros: 2, crits: 0, c4xx: 21, reqs: 6120, p95_seg: 0.14, max_seg: 0.9, severidade: 'Verde' as const },
  { ...base, nome: 'intranet', uptime: 'Up 12 days', erros: 6, crits: 0, c5xx: 1, c4xx: 44, reqs: 3105, p95_seg: 0.22, max_seg: 1.1, severidade: 'Verde' as const },
]

const ALERTAS = [
  { container: 'supply', tipo: 'parado', mensagem: 'supply parou (exit code 137) — sem coleta desde então', criado_em: agora - 3 * 3600 },
]

const ERROS = [
  { id: 501, container: 'prezzo', nivel: 'CRITICAL', linha: 'ORA-00060: deadlock detected while waiting for resource | recalculo_precos worker=3 sku=88412', collected_at: agora - 9, novo: false },
  { id: 500, container: 'prezzo', nivel: 'ERROR', linha: '2026-07-29 14:02:11.404 | ERROR | prezzo.oracle:executar:88 - ORA-01555: snapshot too old', collected_at: agora - 41, novo: false },
  { id: 498, container: 'ecomm', nivel: 'ERROR', linha: 'POST /checkout/pagamento 502 Bad Gateway upstream_timeout=30s pedido=77120', collected_at: agora - 180, novo: false },
]

export const Completa = () => (
  <VisaoGeral
    containers={CONTAINERS}
    alertas={ALERTAS}
    erros={ERROS}
    pollingOk={true}
    atualizadoEm={Date.now() - 8000}
    selecionado={null}
    aoSelecionarContainer={() => {}}
    aoClicarErro={() => {}}
  />
)

export const SemAlertas = () => (
  <VisaoGeral
    containers={CONTAINERS}
    alertas={[]}
    erros={ERROS}
    pollingOk={true}
    atualizadoEm={Date.now() - 8000}
    selecionado="prezzo"
    aoSelecionarContainer={() => {}}
    aoClicarErro={() => {}}
  />
)
