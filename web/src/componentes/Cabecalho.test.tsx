import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { Cabecalho } from './Cabecalho'
import type { ContainerResumo } from '../tipos'

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

describe('Cabecalho', () => {
  afterEach(() => cleanup())

  it('mostra o resumo global derivado dos containers', () => {
    render(
      <Cabecalho
        containers={[
          container({ nome: 'app', severidade: 'Vermelho', erros: 10, crits: 2, reqs: 1000 }),
          container({ nome: 'zen', severidade: 'Verde', erros: 0, crits: 0, reqs: 500 }),
        ]}
        erro={null}
      />,
      { wrapper: MemoryRouter },
    )

    expect(screen.getByText(/1 problema/)).toBeInTheDocument()
    expect(screen.getByText(/2 containers/)).toBeInTheDocument()
    expect(screen.getByText(/1\.5k reqs/)).toBeInTheDocument()
    expect(screen.getByText(/12 erros/)).toBeInTheDocument()
  })

  it('pluraliza "problemas" quando há mais de um', () => {
    render(
      <Cabecalho
        containers={[
          container({ nome: 'app', severidade: 'Vermelho' }),
          container({ nome: 'zen', severidade: 'Amarelo' }),
        ]}
        erro={null}
      />,
      { wrapper: MemoryRouter },
    )
    expect(screen.getByText(/2 problemas/)).toBeInTheDocument()
  })

  it('não mostra "atualizado" na nav — isso é do header de cada tela', () => {
    // Achado 14 da revisão: o "atualizado há Xs" duplicava entre a nav e o
    // header da Visão geral. A spec só quer o resumo global aqui.
    render(<Cabecalho containers={[]} erro={null} />, { wrapper: MemoryRouter })
    expect(screen.queryByText(/atualizado/)).not.toBeInTheDocument()
  })

  it('erro não-nulo mostra o banner de API fora', () => {
    render(<Cabecalho containers={[]} erro="API respondeu 500" />, { wrapper: MemoryRouter })
    expect(screen.getByText(/sem resposta da api/i)).toBeInTheDocument()
  })

  it('erro nulo não mostra o banner', () => {
    render(<Cabecalho containers={[]} erro={null} />, { wrapper: MemoryRouter })
    expect(screen.queryByText(/sem resposta da api/i)).not.toBeInTheDocument()
  })

  it('renderiza um link de nav por rota', () => {
    render(<Cabecalho containers={[]} erro={null} />, { wrapper: MemoryRouter })
    expect(screen.getByText('Visão geral')).toBeInTheDocument()
    expect(screen.getByText('Histórico')).toBeInTheDocument()
    expect(screen.getByText('IA · custos')).toBeInTheDocument()
    expect(screen.getByText('Configuração')).toBeInTheDocument()
  })

  describe('toggle de tema', () => {
    it('não mostra botão quando props de tema não são passadas', () => {
      render(<Cabecalho containers={[]} erro={null} />, { wrapper: MemoryRouter })
      expect(screen.queryByRole('button', { name: /alternar/i })).not.toBeInTheDocument()
    })

    it('mostra botão ◐ quando tema e alternarTema são fornecidos', () => {
      render(
        <Cabecalho containers={[]} erro={null} tema="claro" alternarTema={vi.fn()} />,
        { wrapper: MemoryRouter },
      )
      expect(screen.getByRole('button', { name: /alternar/i })).toBeInTheDocument()
      expect(screen.getByText('◐')).toBeInTheDocument()
    })

    it('clique no botão chama alternarTema', () => {
      const alternar = vi.fn()
      render(
        <Cabecalho containers={[]} erro={null} tema="escuro" alternarTema={alternar} />,
        { wrapper: MemoryRouter },
      )
      fireEvent.click(screen.getByRole('button', { name: /alternar/i }))
      expect(alternar).toHaveBeenCalledOnce()
    })

    it('aria-label reflete o tema atual', () => {
      const { rerender } = render(
        <Cabecalho containers={[]} erro={null} tema="claro" alternarTema={vi.fn()} />,
        { wrapper: MemoryRouter },
      )
      expect(screen.getByRole('button', { name: /escuro/i })).toBeInTheDocument()

      rerender(
        <Cabecalho containers={[]} erro={null} tema="escuro" alternarTema={vi.fn()} />,
      )
      expect(screen.getByRole('button', { name: /claro/i })).toBeInTheDocument()
    })
  })
})
