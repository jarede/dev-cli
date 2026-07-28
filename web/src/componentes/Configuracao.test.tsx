import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { Configuracao } from './Configuracao'
import type { ConfigEfetiva } from '../tipos'

vi.mock('../api', () => ({
  buscarConfig: vi.fn(),
}))
import { buscarConfig } from '../api'
const buscarConfigFalso = vi.mocked(buscarConfig)

function config(parcial: Partial<ConfigEfetiva>): ConfigEfetiva {
  return {
    coleta: {
      intervalo_seg: 45,
      janela_min: 30,
      retencao_horas: 48,
      tail_inicial: 1000,
      db: '',
      ssh: '',
    },
    limiares: { p95_lento_seg: 1.0, taxa_erro_pct: 5.0 },
    servidor: { bind: '0.0.0.0:9191', portal_dir: '/var/lib/dev-cli/portal' },
    ...parcial,
  }
}

describe('Configuracao', () => {
  afterEach(() => {
    cleanup()
    buscarConfigFalso.mockReset()
  })

  it('carrega e mostra a config efetiva do servidor (não valores chumbados)', async () => {
    buscarConfigFalso.mockResolvedValue(config({}))
    render(<Configuracao />)

    // Achado 8: antes os valores vinham de useState() chumbado (30, 8787...)
    // — agora vêm do servidor via /api/config.
    expect(await screen.findByDisplayValue('45')).toBeInTheDocument()
    expect(screen.getByDisplayValue('0.0.0.0:9191')).toBeInTheDocument()
    expect(screen.getByDisplayValue('/var/lib/dev-cli/portal')).toBeInTheDocument()
  })

  it('todos os inputs são somente-leitura', async () => {
    buscarConfigFalso.mockResolvedValue(config({}))
    render(<Configuracao />)
    await screen.findByDisplayValue('45')

    for (const input of screen.getAllByRole('spinbutton')) {
      expect(input).toHaveAttribute('readOnly')
    }
    for (const input of screen.getAllByRole('textbox')) {
      expect(input).toHaveAttribute('readOnly')
    }
  })

  it('ssh vazio marca "docker local" como selecionado', async () => {
    buscarConfigFalso.mockResolvedValue(config({ coleta: { ...config({}).coleta, ssh: '' } }))
    render(<Configuracao />)
    await screen.findByDisplayValue('45')

    expect(screen.getByText('docker local').closest('.seg-opt')).toHaveClass('selected')
  })

  it('ssh preenchido marca "SSH remoto" como selecionado', async () => {
    buscarConfigFalso.mockResolvedValue(
      config({ coleta: { ...config({}).coleta, ssh: 'dev@vm-producao' } }),
    )
    render(<Configuracao />)
    await screen.findByDisplayValue('dev@vm-producao')

    expect(screen.getByText('SSH remoto').closest('.seg-opt')).toHaveClass('selected')
  })

  it('falha na busca mostra o banner de erro', async () => {
    buscarConfigFalso.mockImplementation(async () => {
      throw new Error('API respondeu 500')
    })
    render(<Configuracao />)
    expect(await screen.findByText(/API respondeu 500/)).toBeInTheDocument()
  })
})
