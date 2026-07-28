import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { IaCustos } from './IaCustos'
import type { CustosIa } from '../tipos'

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
      { modelo: 'claude-sonnet-4', provedor: 'anthropic', sessoes: 10, tokens: 1000, custo_usd: 100 },
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

    expect(await screen.findByText('claude-sonnet-4')).toBeInTheDocument()
    expect(screen.getByText(/412\.6M/)).toBeInTheDocument()
    expect(screen.getByText(/14 dias/)).toBeInTheDocument()
  })

  it('Horas com Claude mostra "—" quando claude_disponivel é falso', async () => {
    custosFalso.mockResolvedValue(
      custos({ claude_disponivel: false, claude_horas_mes: 0, claude_horas_por_semana: [] }),
    )
    cambioFalso.mockResolvedValue({ usd_brl: 5.42 })
    render(<IaCustos />)

    await screen.findByText('claude-sonnet-4')
    expect(screen.getByText('—')).toBeInTheDocument()
    // O texto "sem sessões do Claude Code" aparece duas vezes (o KPI e a
    // seção "Horas por semana") — `getAllByText` em vez de `getByText`.
    expect(screen.getAllByText(/sem sessões do Claude Code/).length).toBeGreaterThan(0)
  })

  it('toggle de moeda troca o texto do botão e o valor principal', async () => {
    custosFalso.mockResolvedValue(custos({}))
    cambioFalso.mockResolvedValue({ usd_brl: 5.42 })
    render(<IaCustos />)

    await screen.findByText('claude-sonnet-4')
    // Default é R$ (186.4 * 5.42 = 1010.288 -> "R$ 1.010,29").
    expect(screen.getByText('R$ 1.010,29')).toBeInTheDocument()

    fireEvent.click(screen.getByText('mostrar em US$'))
    expect(screen.getByText('US$ 186.40')).toBeInTheDocument()
  })

  it('banco do OpenCode indisponível mostra o estado vazio', async () => {
    custosFalso.mockResolvedValue(custos({ disponivel: false }))
    cambioFalso.mockResolvedValue({ usd_brl: 5.42 })
    render(<IaCustos />)

    expect(await screen.findByText(/dados não disponíveis/)).toBeInTheDocument()
  })

  it('falha na busca mostra o banner de erro', async () => {
    custosFalso.mockImplementation(async () => {
      throw new Error('API respondeu 500')
    })
    cambioFalso.mockResolvedValue({ usd_brl: 5.42 })
    render(<IaCustos />)

    expect(await screen.findByText(/API respondeu 500/)).toBeInTheDocument()
  })
})
