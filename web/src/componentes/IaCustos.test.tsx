import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { IaCustos } from './IaCustos'
import type { CustosIa } from '../tipos'
import { mesAnterior, mesAtual } from '../formato'

vi.mock('../api', () => ({
  buscarCustosIa: vi.fn(),
  buscarCambio: vi.fn(),
}))
import { buscarCambio, buscarCustosIa } from '../api'
const custosFalso = vi.mocked(buscarCustosIa)
const cambioFalso = vi.mocked(buscarCambio)

function custos(parcial: Partial<CustosIa>): CustosIa {
  return {
    mes: '2026-07',
    disponivel: true,
    tokens: 412_600_000,
    custo_usd: 186.4,
    cache_pct: 86,
    streak_dias: 14,
    melhor_streak_dias: 23,
    heatmap: [
      { dia: 1, intensidade: 0 },
      { dia: 2, intensidade: 5 },
    ],
    offset_semana_dia1: 2,
    modelos: [
      { modelo: 'claude-sonnet-4', provedor: 'anthropic', sessoes: 10, tokens: 1000, custo_usd: 100, tokens_cache: 500, tokens_sem_cache: 500 },
    ],
    claude_disponivel: true,
    claude_horas_mes: 61.33,
    claude_media_horas_dia_ativo: 3.5,
    claude_horas_por_semana: [{ rotulo: '29/06', horas: 18.3 }],
    ...parcial,
  }
}

describe('IaCustos', () => {
  afterEach(() => {
    cleanup()
    custosFalso.mockReset()
    cambioFalso.mockReset()
  })

  it('caminho feliz: mostra KPIs, heatmap e tabela de modelos', async () => {
    custosFalso.mockResolvedValue(custos({}))
    cambioFalso.mockResolvedValue({ usd_brl: 5.42 })
    render(<IaCustos />)

    await screen.findAllByText('claude-sonnet-4')
    expect(screen.getByText(/412\.6M/)).toBeInTheDocument()
    expect(screen.getByText(/14 dias/)).toBeInTheDocument()
  })

  it('Horas com Claude mostra "—" quando claude_disponivel é falso', async () => {
    custosFalso.mockResolvedValue(
      custos({ claude_disponivel: false, claude_horas_mes: 0, claude_horas_por_semana: [] }),
    )
    cambioFalso.mockResolvedValue({ usd_brl: 5.42 })
    render(<IaCustos />)

    await screen.findAllByText('claude-sonnet-4')
    expect(screen.getByText('—')).toBeInTheDocument()
  })

  it('toggle de moeda troca o texto do botão e o valor principal', async () => {
    custosFalso.mockResolvedValue(custos({}))
    cambioFalso.mockResolvedValue({ usd_brl: 5.42 })
    render(<IaCustos />)

    await screen.findAllByText('claude-sonnet-4')
    expect(screen.getByText('R$ 1.010,29')).toBeInTheDocument()

    fireEvent.click(screen.getByText('mostrar em US$'))
    expect(screen.getByText('US$ 186.40')).toBeInTheDocument()
  })

  it('sem dados em nenhuma fonte mostra o estado vazio', async () => {
    custosFalso.mockResolvedValue(custos({ disponivel: false }))
    cambioFalso.mockResolvedValue({ usd_brl: 5.42 })
    render(<IaCustos />)

    expect(await screen.findByText(/nenhuma das fontes tem dados neste mês/i)).toBeInTheDocument()
  })

  it('falha na busca mostra o banner de erro', async () => {
    custosFalso.mockRejectedValue(new Error('API respondeu 500'))
    cambioFalso.mockResolvedValue({ usd_brl: 5.42 })
    render(<IaCustos />)

    expect(await screen.findByText(/API respondeu 500/)).toBeInTheDocument()
  })

  it('seta ‹ busca o mês anterior e › fica desabilitada no mês atual', async () => {
    custosFalso.mockResolvedValue(custos({}))
    cambioFalso.mockResolvedValue({ usd_brl: 5.42 })
    render(<IaCustos />)

    await screen.findAllByText('claude-sonnet-4')
    const voltar = screen.getByRole('button', { name: /mês anterior/i })
    const avancar = screen.getByRole('button', { name: /mês seguinte/i })
    expect(avancar).toBeDisabled()
    fireEvent.click(voltar)
    await waitFor(() => {
      const chamadas = custosFalso.mock.calls.map((c) => String(c))
      expect(chamadas.some((u) => u.includes(mesAnterior(mesAtual())))).toBe(true)
    })
  })

  it('segmented de fonte refaz o fetch com o valor escolhido', async () => {
    custosFalso.mockResolvedValue(custos({}))
    cambioFalso.mockResolvedValue({ usd_brl: 5.42 })
    render(<IaCustos />)

    fireEvent.click(await screen.findByLabelText('OpenCode'))
    await waitFor(() => {
      // Segundo argumento das chamadas: fonte passada para buscarCustosIa.
      const fontes = custosFalso.mock.calls.map((c) => c[1])
      expect(fontes).toContain('opencode')
    })
  })

  it('subtítulo reflete a fonte "claude"', async () => {
    custosFalso.mockResolvedValue(custos({}))
    cambioFalso.mockResolvedValue({ usd_brl: 5.42 })
    render(<IaCustos />)

    await screen.findAllByText('claude-sonnet-4')
    fireEvent.click(screen.getByLabelText('Claude'))
    await waitFor(() => {
      expect(screen.getByText('Claude · câmbio R$ 5,42')).toBeInTheDocument()
    })
  })
})