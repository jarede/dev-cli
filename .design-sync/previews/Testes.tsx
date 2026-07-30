// Preview: tela Testes · e2e — lista de suítes (pipelines CLI). As 4 suítes
// do handoff (§5): sanidade (passa), prezzo (falha de validação), ecomm
// (falha de execução), bapi (passa). Expandir/rodar são interações — o
// preview mostra a lista carregada (estado estático real da tela).
import { Testes } from 'web'

const SUITES = [
  {
    id: 'sanidade-escrita',
    nome: 'sanidade · escrita de arquivo',
    timeout_etapa_seg: 10,
    passos: [
      { tipo: 'exec' as const, cmd: 'echo oi > oi.txt' },
      { tipo: 'valida' as const, cmd: 'test -f oi.txt && echo existe', esperado: 'existe' },
      { tipo: 'valida' as const, cmd: "grep -c '^oi$' oi.txt", esperado: '1' },
    ],
  },
  {
    id: 'prezzo-recalculo',
    nome: 'prezzo · recalcular preços',
    timeout_etapa_seg: 60,
    passos: [
      { tipo: 'exec' as const, cmd: 'dev-cli precos recalcular --loja 12' },
      { tipo: 'valida' as const, cmd: 'dev-cli precos contar --loja 12', esperado: '1842' },
      { tipo: 'valida' as const, cmd: 'dev-cli precos status --loja 12', esperado: 'concluido' },
      { tipo: 'valida' as const, cmd: 'dev-cli precos auditar --loja 12', esperado: 'ok' },
    ],
  },
  {
    id: 'ecomm-checkout',
    nome: 'ecomm · checkout de ponta a ponta',
    timeout_etapa_seg: 30,
    passos: [
      { tipo: 'exec' as const, cmd: 'dev-cli ecomm criar-pedido --sku 88412' },
      { tipo: 'exec' as const, cmd: 'curl -sf http://ecomm/pagamentos/ultimo' },
      { tipo: 'valida' as const, cmd: 'dev-cli ecomm pedido-status --ultimo', esperado: 'pago' },
      { tipo: 'valida' as const, cmd: 'dev-cli ecomm estoque --sku 88412', esperado: '41' },
    ],
  },
  {
    id: 'bapi-webhook',
    nome: 'bapi · webhook de parceiro',
    timeout_etapa_seg: 15,
    passos: [
      { tipo: 'exec' as const, cmd: 'dev-cli bapi simular-webhook --parceiro acme' },
      { tipo: 'valida' as const, cmd: 'dev-cli bapi ultimo-evento --parceiro acme', esperado: 'recebido' },
    ],
  },
]

const fetchOriginal = window.fetch.bind(window)
window.fetch = (async (recurso: RequestInfo | URL, init?: RequestInit) => {
  const url = String(recurso)
  if (url.includes('/api/testes/suites') && !url.includes('/rodar')) {
    return new Response(JSON.stringify(SUITES), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }
  if (url.includes('/api/testes/')) {
    return new Response(JSON.stringify([]), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }
  return fetchOriginal(recurso, init)
}) as typeof fetch

export const QuatroSuites = () => <Testes />
