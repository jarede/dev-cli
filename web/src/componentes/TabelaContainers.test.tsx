import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import { TabelaContainers } from './TabelaContainers'
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

describe('TabelaContainers', () => {
  it('lista vazia mostra a mensagem de espera', () => {
    render(<TabelaContainers containers={[]} selecionado={null} aoSelecionar={() => {}} />)
    expect(screen.getByText(/aguarde a primeira coleta/)).toBeInTheDocument()
  })

  it('renderiza nome, p95 formatado e severidade', () => {
    const c = container({ nome: 'qa-prezzo-1', p95_seg: 2.32, severidade: 'Vermelho' })
    render(<TabelaContainers containers={[c]} selecionado={null} aoSelecionar={() => {}} />)
    expect(screen.getByText('qa-prezzo-1')).toBeInTheDocument()
    expect(screen.getByText('2.32s')).toBeInTheDocument()
    expect(screen.getByTitle('Vermelho')).toBeInTheDocument()
  })

  it('clicar numa linha chama aoSelecionar com o nome', () => {
    const aoSelecionar = vi.fn()
    render(
      <TabelaContainers
        containers={[container({ nome: 'app' })]}
        selecionado={null}
        aoSelecionar={aoSelecionar}
      />,
    )
    fireEvent.click(screen.getByText('app'))
    expect(aoSelecionar).toHaveBeenCalledWith('app')
  })

  it('container parado mostra "parado" no status', () => {
    const c = container({ nome: 'morto', status: 'stopped', severidade: 'Parado' })
    render(<TabelaContainers containers={[c]} selecionado={null} aoSelecionar={() => {}} />)
    expect(screen.getByText('parado')).toBeInTheDocument()
  })
})
