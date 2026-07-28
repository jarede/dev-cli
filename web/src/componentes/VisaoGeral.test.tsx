import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { VisaoGeral } from './VisaoGeral'
import type { ContainerResumo } from '../tipos'
import type { ErroLogComDestaque } from './FeedErros'

function container(parcial: Partial<ContainerResumo>): ContainerResumo {
  return {
    nome: 'app',
    status: 'running',
    uptime: 'Up 1 day',
    erros: 0,
    crits: 0,
    c5xx: 0,
    c4xx: 0,
    reqs: 0,
    p95_seg: null,
    max_seg: null,
    total_linhas: 0,
    ultima_coleta: 0,
    severidade: 'Verde',
    ...parcial,
  }
}

const semAcao = {
  alertas: [],
  erros: [] as ErroLogComDestaque[],
  selecionado: null,
  aoSelecionarContainer: () => {},
  aoClicarErro: () => {},
}

describe('VisaoGeral', () => {
  afterEach(() => cleanup())

  it('mostra o header com o timestamp "atualizado há Xs"', () => {
    render(
      <VisaoGeral
        containers={[]}
        {...semAcao}
        pollingOk={true}
        atualizadoEm={Date.now()}
      />,
    )
    expect(screen.getByText('Visão geral')).toBeInTheDocument()
    expect(screen.getByText(/atualizado há/)).toBeInTheDocument()
  })

  it('sem primeira coleta ainda mostra o placeholder de espera', () => {
    render(<VisaoGeral containers={[]} {...semAcao} pollingOk={true} atualizadoEm={null} />)
    expect(screen.getByText(/aguardando primeira coleta/)).toBeInTheDocument()
  })

  it('renderiza a tabela de containers e repassa o clique de linha', () => {
    const aoSelecionarContainer = vi.fn()
    render(
      <VisaoGeral
        containers={[container({ nome: 'qa-prezzo-1' })]}
        {...semAcao}
        aoSelecionarContainer={aoSelecionarContainer}
        pollingOk={true}
        atualizadoEm={null}
      />,
    )
    fireEvent.click(screen.getByText('qa-prezzo-1'))
    expect(aoSelecionarContainer).toHaveBeenCalledWith('qa-prezzo-1')
  })

  it('clique num item do feed repassa container e nível', () => {
    const aoClicarErro = vi.fn()
    const erro: ErroLogComDestaque = {
      id: 1,
      container: 'qa-prezzo-1',
      nivel: 'ERROR',
      linha: 'deu ruim',
      collected_at: Math.floor(Date.now() / 1000),
      novo: false,
    }
    render(
      <VisaoGeral
        containers={[]}
        {...semAcao}
        erros={[erro]}
        aoClicarErro={aoClicarErro}
        pollingOk={true}
        atualizadoEm={null}
      />,
    )
    fireEvent.click(screen.getByText(/deu ruim/))
    expect(aoClicarErro).toHaveBeenCalledWith('qa-prezzo-1', 'ERROR')
  })

  it('achado 3: a bolinha "ao vivo" segue pollingOk, não a quantidade de erros', () => {
    // Sistema saudável (zero erros) com polling funcionando: "ao vivo"
    // (verde), não "parado". Antes o componente passava
    // `aoVivo={erros.length > 0}`, que invertia esse sinal.
    const { container: c1 } = render(
      <VisaoGeral containers={[]} {...semAcao} pollingOk={true} atualizadoEm={null} />,
    )
    expect(c1.querySelector('.bolinha-aovivo')).not.toHaveClass('parado')
    cleanup()

    // API fora do ar (pollingOk false), mesmo com itens no feed: "parado".
    const erro: ErroLogComDestaque = {
      id: 1,
      container: 'app',
      nivel: 'ERROR',
      linha: 'x',
      collected_at: 1,
      novo: false,
    }
    const { container: c2 } = render(
      <VisaoGeral containers={[]} {...semAcao} erros={[erro]} pollingOk={false} atualizadoEm={null} />,
    )
    expect(c2.querySelector('.bolinha-aovivo')).toHaveClass('parado')
  })
})
