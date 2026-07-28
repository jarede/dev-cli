// Card de Alertas (containers que pararam/reiniciaram) — apresentacional:
// a App busca os dados; sem alertas, não renderiza nada.

import type { Alerta } from '../tipos'
import { formatarHaQuanto } from '../formato'

export function ListaAlertas({ alertas }: { alertas: Alerta[] }) {
  if (alertas.length === 0) return null

  return (
    <section className="card card-acento">
      <div className="card-kicker">Alertas</div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 7 }}>
        {alertas.map((a, i) => (
          <div className="alerta-item" key={i}>
            <span className="marca">§</span>
            <span className="msg">{a.mensagem}</span>
            <span className="quando">{formatarHaQuanto(a.criado_em)}</span>
          </div>
        ))}
      </div>
    </section>
  )
}
