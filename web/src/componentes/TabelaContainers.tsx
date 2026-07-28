// A tabela de containers da Visão geral: containers JÁ ordenados pela
// API (piores primeiro) — este componente só APRESENTA. Selecionar uma
// linha abre o drill-down (PainelContainer) via callback do pai.
// Componente "burro"/apresentacional: recebe tudo por props, não busca
// dados — o que o torna trivial de testar.
// docs: https://react.dev/learn/passing-props-to-a-component

import type { ContainerResumo } from '../tipos'
import { COR_SEVERIDADE, formatarNumero, formatarSegundos } from '../formato'

interface Props {
  containers: ContainerResumo[]
  selecionado: string | null
  aoSelecionar: (nome: string) => void
}

export function TabelaContainers({ containers, selecionado, aoSelecionar }: Props) {
  if (containers.length === 0) {
    return <p className="vazio">nenhum container conhecido ainda — aguarde a primeira coleta</p>
  }

  return (
    <table className="table">
      <thead>
        <tr>
          <th style={{ width: 18 }}></th>
          <th>container</th>
          <th>status</th>
          <th className="num">err</th>
          <th className="num">crit</th>
          <th className="num">5xx</th>
          <th className="num">4xx</th>
          <th className="num">p95</th>
          <th className="num">máx</th>
          <th className="num">reqs</th>
        </tr>
      </thead>
      <tbody>
        {containers.map((c) => {
          // `err > 50` e `crit > 0` ganham cor vermelha (regra do design).
          const corErr = c.erros > 50 ? 'vermelho' : undefined
          const corCrit = c.crits > 0 ? 'vermelho' : undefined
          return (
            <tr
              key={c.nome}
              className={c.nome === selecionado ? 'selecionada' : undefined}
              onClick={() => aoSelecionar(c.nome)}
            >
              <td>
                <span
                  className="bolinha"
                  style={{ background: COR_SEVERIDADE[c.severidade] }}
                  title={c.severidade}
                />
              </td>
              <td className="nome">{c.nome}</td>
              <td className="status">{c.status === 'stopped' ? 'parado' : c.uptime || c.status}</td>
              <td className={`num ${corErr ?? ''}`.trim()}>{formatarNumero(c.erros)}</td>
              <td className={`num ${corCrit ?? ''}`.trim()}>{formatarNumero(c.crits)}</td>
              <td className="num">{formatarNumero(c.c5xx)}</td>
              <td className="num">{formatarNumero(c.c4xx)}</td>
              <td className="num">{formatarSegundos(c.p95_seg)}</td>
              <td className="num">{formatarSegundos(c.max_seg)}</td>
              <td className="num">{formatarNumero(c.reqs)}</td>
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}
