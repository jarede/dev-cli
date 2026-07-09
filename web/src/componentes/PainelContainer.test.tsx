import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { PainelContainer } from './PainelContainer'

vi.mock('../api', () => ({
  buscarLinhas: vi.fn(),
}))
import { buscarLinhas } from '../api'
const buscarLinhasFalso = vi.mocked(buscarLinhas)

describe('PainelContainer', () => {
  afterEach(() => {
    cleanup()
    buscarLinhasFalso.mockReset()
  })
  beforeEach(() => {
    buscarLinhasFalso.mockResolvedValue([])
  })

  it('busca e mostra as linhas do container ao montar', async () => {
    buscarLinhasFalso.mockResolvedValue([
      { nivel: 'ERROR', linha: 'deu ruim na fatura', collected_at: 1 },
    ])
    render(<PainelContainer nome="qa-prezzo-1" aoFechar={() => {}} />)

    expect(await screen.findByText(/deu ruim na fatura/)).toBeInTheDocument()
    expect(buscarLinhasFalso).toHaveBeenCalledWith('qa-prezzo-1', undefined)
  })

  it('trocar o nível refaz a busca com o filtro', async () => {
    buscarLinhasFalso.mockResolvedValue([])
    render(<PainelContainer nome="app" aoFechar={() => {}} />)
    await screen.findByText(/nenhuma linha/)

    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'ERROR' } })

    expect(await screen.findByText(/nenhuma linha/)).toBeInTheDocument()
    expect(buscarLinhasFalso).toHaveBeenLastCalledWith('app', 'ERROR')
  })

  it('botão fechar chama aoFechar', async () => {
    buscarLinhasFalso.mockResolvedValue([])
    const aoFechar = vi.fn()
    render(<PainelContainer nome="app" aoFechar={aoFechar} />)
    await screen.findByText(/nenhuma linha/)

    fireEvent.click(screen.getByText('fechar'))
    expect(aoFechar).toHaveBeenCalled()
  })

  it('erro da API aparece no painel', async () => {
    buscarLinhasFalso.mockImplementation(async () => {
      throw new Error('API respondeu 500')
    })
    render(<PainelContainer nome="app" aoFechar={() => {}} />)

    expect(await screen.findByText(/API respondeu 500/)).toBeInTheDocument()
  })
})
