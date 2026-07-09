import type { ContainerResumo } from '../tipos'
import { formatarHaQuanto, formatarNumero } from '../formato'

interface Props {
  containers: ContainerResumo[]
  erro: string | null
  atualizadoEm: number | null
}

export function Cabecalho({ containers, erro, atualizadoEm }: Props) {
  const problemas = containers.filter((c) => c.severidade !== 'Verde').length
  const reqs = containers.reduce((soma, c) => soma + c.reqs, 0)
  const erros = containers.reduce((soma, c) => soma + c.erros + c.crits, 0)

  return (
    <>
      <header className="cabecalho">
        <h1>dev-cli · portal</h1>
        <span className="resumo">
          {problemas} problema{problemas === 1 ? '' : 's'} · {containers.length} containers ·{' '}
          {formatarNumero(reqs)} reqs · {formatarNumero(erros)} erros
          {atualizadoEm !== null &&
            ` · atualizado ${formatarHaQuanto(Math.floor(atualizadoEm / 1000))}`}
        </span>
      </header>
      {erro !== null && (
        <p className="erro-conexao">
          ⚠ sem resposta da API — o dev-server está rodando? ({erro})
        </p>
      )}
    </>
  )
}
