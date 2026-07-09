// A tabela do dashboard: containers JÁ ordenados pela API (piores
// primeiro) — este componente só APRESENTA. Selecionar uma linha abre o
// drill-down (PainelContainer, Task 4) via callback do pai.
// Componente "burro"/apresentacional: recebe tudo por props, não busca
// dados — o que o torna trivial de testar.
// docs: https://react.dev/learn/passing-props-to-a-component

import type { ContainerResumo, Severidade } from '../tipos'
import { formatarNumero, formatarSegundos } from '../formato'

interface Props {
  containers: ContainerResumo[]
  selecionado: string | null
  aoSelecionar: (nome: string) => void
}

/// Cor de cada severidade (variáveis do index.css).
/// Record<K, V>: objeto com TODAS as chaves de Severidade — o TS acusa se
/// uma variante nova ficar sem cor.
/// docs: https://www.typescriptlang.org/docs/handbook/utility-types.html#recordkeys-type
const COR_SEVERIDADE: Record<Severidade, string> = {
  Verde: 'var(--verde)',
  Amarelo: 'var(--amarelo)',
  Vermelho: 'var(--vermelho)',
  Parado: 'var(--parado)',
}

export function TabelaContainers({ containers, selecionado, aoSelecionar }: Props) {
  if (containers.length === 0) {
    return <p className="vazio">nenhum container conhecido ainda — aguarde a primeira coleta</p>
  }

  return (
    <table className="tabela-containers">
      <thead>
        <tr>
          <th></th>
          <th>container</th>
          <th>status</th>
          <th>err</th>
          <th>crit</th>
          <th>5xx</th>
          <th>4xx</th>
          <th>p95</th>
          <th>máx</th>
          <th>reqs</th>
        </tr>
      </thead>
      <tbody>
        {containers.map((c) => (
          // `key`: identidade estável de cada linha para o React
          // reconciliar a lista sem recriar DOM à toa.
          // docs: https://react.dev/learn/rendering-lists#keeping-list-items-in-order-with-key
          <tr
            key={c.nome}
            className={c.nome === selecionado ? 'selecionada' : ''}
            onClick={() => aoSelecionar(c.nome)}
          >
            <td>
              <span
                className="bolinha"
                style={{ background: COR_SEVERIDADE[c.severidade] }}
                title={c.severidade}
              />
            </td>
            <td>{c.nome}</td>
            <td>{c.status === 'stopped' ? 'parado' : c.uptime || c.status}</td>
            <td>{formatarNumero(c.erros)}</td>
            <td>{formatarNumero(c.crits)}</td>
            <td>{formatarNumero(c.c5xx)}</td>
            <td>{formatarNumero(c.c4xx)}</td>
            <td>{formatarSegundos(c.p95_seg)}</td>
            <td>{formatarSegundos(c.max_seg)}</td>
            <td>{formatarNumero(c.reqs)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
