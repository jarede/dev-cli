import type { ModeloCusto } from '../tipos'
import { corDoModelo } from '../formato'

export interface FatiaPizza {
  modelo: string
  pct: number
  cor: string
}

export function fatiasPizza(modelos: ModeloCusto[]): FatiaPizza[] {
  const comTokens = modelos.filter((m) => m.tokens > 0)
  const total = comTokens.reduce((soma, m) => soma + m.tokens, 0)
  if (total === 0) return []
  return comTokens
    .slice()
    .sort((a, b) => b.tokens - a.tokens)
    .map((m, i) => ({
      modelo: m.modelo,
      pct: (m.tokens * 100) / total,
      cor: corDoModelo(m.modelo, i),
    }))
}

export function PizzaModelos({ modelos }: { modelos: ModeloCusto[] }) {
  const fatias = fatiasPizza(modelos)
  if (fatias.length === 0) return null

  let acumulado = 0
  const setores = fatias.map((f) => {
    const inicio = acumulado
    acumulado += f.pct
    return `${f.cor} ${inicio}% ${acumulado}%`
  })

  return (
    <div className="pizza-wrap">
      <div
        className="pizza"
        role="img"
        aria-label={`Distribuição de tokens: ${fatias.map((f) => `${f.modelo} ${f.pct.toFixed(0)}%`).join(', ')}`}
        style={{ background: `conic-gradient(${setores.join(', ')})` }}
      />
      <ul className="pizza-legenda">
        {fatias.map((f) => (
          <li key={f.modelo}>
            <span className="pizza-cor" style={{ background: f.cor }} />
            <span className="pizza-nome">{f.modelo}</span>
            <span className="pizza-pct">{f.pct.toFixed(1).replace('.', ',')}%</span>
          </li>
        ))}
      </ul>
    </div>
  )
}