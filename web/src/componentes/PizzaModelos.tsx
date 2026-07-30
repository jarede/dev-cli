import type { ModeloCusto } from '../tipos'
import { corDoModelo } from '../formato'

export interface FatiaPizza {
  modelo: string
  pct: number
  cor: string
}

export function fatiasPizza(modelos: ModeloCusto[]): FatiaPizza[] {
  // A pizza mede MODELO, não fonte: em `fonte=ambos` o mesmo modelo pode
  // vir duas vezes na lista (provedores `anthropic` e `claude-code`, por
  // exemplo), e sem agregar aqui geraríamos duas fatias da mesma cor com
  // `key` duplicada no React. `Map` preserva a soma por nome antes de
  // ordenar/percentualizar — a tabela ao lado (`TabelaModelos`) continua
  // mostrando as linhas por provedor; só a pizza agrega.
  const tokensPorModelo = new Map<string, number>()
  for (const m of modelos) {
    if (m.tokens <= 0) continue
    tokensPorModelo.set(m.modelo, (tokensPorModelo.get(m.modelo) ?? 0) + m.tokens)
  }
  const total = [...tokensPorModelo.values()].reduce((soma, t) => soma + t, 0)
  if (total === 0) return []
  return [...tokensPorModelo.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([modelo, tokens], i) => ({
      modelo,
      pct: (tokens * 100) / total,
      cor: corDoModelo(modelo, i),
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