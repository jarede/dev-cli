// Alertas recentes (containers que pararam/reiniciaram) — apresentacional:
// a App busca os dados; sem alertas, não ocupa espaço nenhum.

import type { Alerta } from '../tipos'
import { formatarHaQuanto } from '../formato'

export function ListaAlertas({ alertas }: { alertas: Alerta[] }) {
  if (alertas.length === 0) return null

  return (
    <ul className="alertas">
      {alertas.map((a, i) => (
        <li key={i}>
          ⚠ {a.mensagem} · {formatarHaQuanto(a.criado_em)}
        </li>
      ))}
    </ul>
  )
}
