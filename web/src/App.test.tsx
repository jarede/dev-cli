import { beforeEach, describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import App from './App'

vi.mock('./api', () => ({
  buscarContainers: vi.fn(),
  buscarAlertas: vi.fn(),
}))
import { buscarAlertas, buscarContainers } from './api'
const containersFalso = vi.mocked(buscarContainers)
const alertasFalso = vi.mocked(buscarAlertas)

describe('App', () => {
  beforeEach(() => {
    containersFalso.mockReset()
    alertasFalso.mockReset()
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

    render(<App />)

    expect(await screen.findByText('qa-prezzo-1')).toBeInTheDocument()
    expect(screen.getByText(/1 problema/)).toBeInTheDocument()
    expect(screen.getByText(/Container 'morto' parou/)).toBeInTheDocument()
  })

  it('falha da API mostra o banner de erro', async () => {
    containersFalso.mockRejectedValue(new Error('API respondeu 500'))
    alertasFalso.mockResolvedValue([])

    render(<App />)

    expect(await screen.findByText(/sem resposta da API/)).toBeInTheDocument()
  })
})
