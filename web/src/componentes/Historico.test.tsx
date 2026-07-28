import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { Historico } from './Historico'

vi.mock('../api', () => ({
  buscarHistorico: vi.fn(),
}))
import { buscarHistorico } from '../api'
const buscarHistoricoFalso = vi.mocked(buscarHistorico)

describe('Historico', () => {
  afterEach(() => {
    cleanup()
    buscarHistoricoFalso.mockReset()
  })

  it('busca 24h ao montar e mostra o strip por container', async () => {
    buscarHistoricoFalso.mockResolvedValue([
      {
        nome: 'qa-prezzo-1',
        total: 8,
        horas: [
          { hora: 1_720_015_200, quantidade: 5, intensidade: 4 },
          { hora: 1_720_011_600, quantidade: 3, intensidade: 2 },
        ],
      },
    ])
    render(<Historico />)

    expect(await screen.findByText('qa-prezzo-1')).toBeInTheDocument()
    expect(buscarHistoricoFalso).toHaveBeenCalledWith(24)
    expect(screen.getByText('8')).toBeInTheDocument()
  })

  it('tooltip da célula inclui a hora formatada (achado 15)', async () => {
    buscarHistoricoFalso.mockResolvedValue([
      {
        nome: 'app',
        total: 5,
        horas: [{ hora: 1_720_015_200, quantidade: 5, intensidade: 3 }],
      },
    ])
    const { container } = render(<Historico />)
    await screen.findByText('app')

    const celula = container.querySelector('.historico-celula')
    expect(celula).toHaveAttribute('title', '14h · 5 erros/críticos')
  })

  it('nenhum container conhecido mostra o estado vazio', async () => {
    buscarHistoricoFalso.mockResolvedValue([])
    render(<Historico />)
    expect(await screen.findByText(/nenhum container conhecido/)).toBeInTheDocument()
  })

  it('falha da API mostra o banner de erro', async () => {
    buscarHistoricoFalso.mockImplementation(async () => {
      throw new Error('API respondeu 500')
    })
    render(<Historico />)
    expect(await screen.findByText(/API respondeu 500/)).toBeInTheDocument()
  })
})
