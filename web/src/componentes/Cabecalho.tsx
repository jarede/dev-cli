// Nav sticky no topo do portal: marca + 4 rotas (NavLinks do react-router)
// + resumo global derivado dos containers. O `aria-current="page"` é
// aplicado automaticamente pelo NavLink quando a rota casa.
//
// A spec (§1 do handoff) põe SÓ o resumo global aqui ("N problemas · 5
// containers · 36.9k reqs · 277 erros") — o "atualizado há Xs" é do header
// de CADA tela (ver `VisaoGeral.tsx`), não da nav. Ter os dois lugares
// mostrando "atualizado" era redundante e podia divergir visualmente
// (mesmo timestamp, formatado duas vezes na tela).

import { NavLink } from 'react-router-dom'
import type { ContainerResumo } from '../tipos'
import { formatarNumero } from '../formato'
import { ROTAS } from '../rotas'

interface Props {
  containers: ContainerResumo[]
  /** Mensagem de erro da última busca; null = tudo bem. */
  erro: string | null
  /** Tema atual ('claro' | 'escuro') para o toggle visual. Opcional para não quebrar usos existentes. */
  tema?: 'claro' | 'escuro'
  /** Função para alternar o tema. Opcional para não quebrar usos existentes. */
  alternarTema?: () => void
}

export function Cabecalho({ containers, erro, tema, alternarTema }: Props) {
  const problemas = containers.filter((c) => c.severidade !== 'Verde').length
  const reqs = containers.reduce((soma, c) => soma + c.reqs, 0)
  const erros = containers.reduce((soma, c) => soma + c.erros + c.crits, 0)

  return (
    <>
      <nav className="nav">
        <NavLink to="/" className="nav-brand" end>
          dev-cli · portal
        </NavLink>
        {ROTAS.map((r) => (
          <NavLink
            key={r.caminho}
            to={r.caminho}
            end={r.fim}
            className="link"
          >
            {r.rotulo}
          </NavLink>
        ))}
        <span className="resumo">
          {problemas} problema{problemas === 1 ? '' : 's'} · {containers.length} containers ·{' '}
          {formatarNumero(reqs)} reqs · {formatarNumero(erros)} erros
        </span>
        {alternarTema !== undefined && (
          <button
            className="btn btn-ghost"
            onClick={alternarTema}
            aria-label={`Alternar para tema ${tema === 'escuro' ? 'claro' : 'escuro'}`}
            title={`Alternar para tema ${tema === 'escuro' ? 'claro' : 'escuro'}`}
            type="button"
          >
            ◐
          </button>
        )}
      </nav>
      {erro !== null && (
        <div className="banner-api-fora">
          Sem resposta da API — o dev-server está rodando? Mostrando os últimos dados conhecidos.
        </div>
      )}
    </>
  )
}
