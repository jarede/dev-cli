// Preview: feed "Erros ao vivo" — linhas de log no estilo Loguru/Oracle do
// Prezzo (handoff §Dados). `novo: true` no item mais recente dispara a
// animação pulseNovo; a bolinha do título indica polling ok (verde) / parado.
import { FeedErros } from 'web'

const agora = Math.floor(Date.now() / 1000)

const ERROS = [
  {
    id: 501,
    container: 'prezzo',
    nivel: 'CRITICAL',
    linha:
      'ORA-00060: deadlock detected while waiting for resource | recalculo_precos worker=3 sku=88412',
    collected_at: agora - 9,
    novo: true,
  },
  {
    id: 500,
    container: 'prezzo',
    nivel: 'ERROR',
    linha:
      '2026-07-29 14:02:11.404 | ERROR | prezzo.oracle:executar:88 - ORA-01555: snapshot too old: rollback segment number 9',
    collected_at: agora - 41,
    novo: false,
  },
  {
    id: 498,
    container: 'ecomm',
    nivel: 'ERROR',
    linha: 'POST /checkout/pagamento 502 Bad Gateway upstream_timeout=30s pedido=77120',
    collected_at: agora - 3 * 60,
    novo: false,
  },
  {
    id: 497,
    container: 'prezzo',
    nivel: 'ERROR',
    linha:
      '2026-07-29 13:58:02.117 | ERROR | prezzo.filas:consumir:41 - timeout esperando lock da fila recalculo (30s)',
    collected_at: agora - 7 * 60,
    novo: false,
  },
]

export const AoVivo = () => (
  <FeedErros erros={ERROS} aoClicar={() => {}} aoVivo={true} />
)

export const PollingParado = () => (
  <FeedErros
    erros={ERROS.map((e) => ({ ...e, novo: false }))}
    aoClicar={() => {}}
    aoVivo={false}
  />
)

export const Vazio = () => <FeedErros erros={[]} aoClicar={() => {}} aoVivo={true} />
