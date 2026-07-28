import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import App from './App'

vi.mock('./api', () => ({
  buscarContainers: vi.fn(),
  buscarAlertas: vi.fn(),
  buscarErros: vi.fn(),
}))
import { buscarAlertas, buscarContainers, buscarErros } from './api'
const containersFalso = vi.mocked(buscarContainers)
const alertasFalso = vi.mocked(buscarAlertas)
const errosFalso = vi.mocked(buscarErros)

describe('App', () => {
  beforeEach(() => {
    containersFalso.mockReset()
    alertasFalso.mockReset()
    errosFalso.mockReset()
    containersFalso.mockResolvedValue([])
    alertasFalso.mockResolvedValue([])
    errosFalso.mockResolvedValue([])
  })

  it('carrega e mostra containers e alertas', async () => {
    containersFalso.mockResolvedValue([
      {
        nome: 'qa-prezzo-1',
        status: 'running',
        uptime: 'Up 2 days',
        erros: 31,
        crits: 2,
        c5xx: 12,
        c4xx: 4,
        reqs: 412,
        p95_seg: 2.31,
        max_seg: 8.1,
        total_linhas: 999,
        ultima_coleta: 1,
        severidade: 'Vermelho',
      },
    ])
    alertasFalso.mockResolvedValue([
      { container: 'morto', tipo: 'stopped', mensagem: "Container 'morto' parou", criado_em: 1 },
    ])

    render(<App />, { wrapper: MemoryRouter })

    expect(await screen.findByText('qa-prezzo-1')).toBeInTheDocument()
    expect(screen.getByText(/1 problema/)).toBeInTheDocument()
    expect(screen.getByText(/Container 'morto' parou/)).toBeInTheDocument()
  })

  it('falha da API mostra o banner de erro', async () => {
    containersFalso.mockRejectedValue(new Error('API respondeu 500'))
    alertasFalso.mockResolvedValue([])

    render(<App />, { wrapper: MemoryRouter })

    expect(await screen.findByText(/sem resposta da api/i)).toBeInTheDocument()
  })

  it('avança o cursor entre polls ao buscar erros', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    errosFalso
      .mockResolvedValueOnce([
        {
          id: 5,
          container: 'app',
          nivel: 'ERROR',
          linha: 'primeiro',
          collected_at: 100,
        },
      ])
      .mockResolvedValueOnce([
        {
          id: 8,
          container: 'zen',
          nivel: 'CRITICAL',
          linha: 'segundo',
          collected_at: 200,
        },
      ])
      .mockResolvedValue([])

    render(<App />, { wrapper: MemoryRouter })

    // Primeira carga: desde_id 0.
    await waitFor(() => {
      expect(errosFalso).toHaveBeenCalledWith(0)
    })
    expect(await screen.findByText('primeiro')).toBeInTheDocument()

    // Avança o intervalo de polling (15s).
    await vi.advanceTimersByTimeAsync(15_000)

    await waitFor(() => {
      // Segunda chamada usa o maior id da primeira resposta.
      expect(errosFalso).toHaveBeenLastCalledWith(5)
    })
    expect(await screen.findByText('segundo')).toBeInTheDocument()

    vi.useRealTimers()
  })
})
