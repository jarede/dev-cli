import { describe, expect, it } from 'vitest'
import {
  corNivel,
  formatarHaQuanto,
  formatarHora,
  formatarHorasMinutos,
  formatarMoeda,
  formatarNumero,
  formatarNumeroCompacto,
  formatarSegundos,
  intensidadeParaCor,
} from './formato'

describe('formatarSegundos', () => {
  it('formata com duas casas e sufixo s', () => {
    expect(formatarSegundos(1.234)).toBe('1.23s')
    expect(formatarSegundos(0)).toBe('0.00s')
  })
  it('null vira travessão (sem requests na janela)', () => {
    expect(formatarSegundos(null)).toBe('—')
  })
})

describe('formatarNumero', () => {
  it('abrevia milhares', () => {
    expect(formatarNumero(1234)).toBe('1.2k')
  })
  it('mantém números pequenos', () => {
    expect(formatarNumero(999)).toBe('999')
    expect(formatarNumero(0)).toBe('0')
  })
})

describe('formatarHaQuanto', () => {
  // agoraMs fixo: teste determinístico, sem depender do relógio real.
  const agora = 1_000_000_000 * 1000 // ts unix 1_000_000_000 em ms

  it('segundos, minutos e horas', () => {
    expect(formatarHaQuanto(1_000_000_000 - 12, agora)).toBe('há 12s')
    expect(formatarHaQuanto(1_000_000_000 - 3 * 60, agora)).toBe('há 3min')
    expect(formatarHaQuanto(1_000_000_000 - 2 * 3600, agora)).toBe('há 2h')
  })
  it('zero ou negativo vira "nunca" (container nunca coletado)', () => {
    expect(formatarHaQuanto(0, agora)).toBe('nunca')
  })
})

describe('formatarMoeda', () => {
  it('US$ mantém 2 casas decimais com ponto', () => {
    expect(formatarMoeda(186.4, 5.42, 'US$')).toBe('US$ 186.40')
  })
  it('R$ troca ponto por vírgula (padrão pt-br)', () => {
    // 186.4 * 5.42 = 1010.288 → formatado como 1.010,29
    expect(formatarMoeda(186.4, 5.42, 'R$')).toBe('R$ 1.010,29')
  })
  it('aceita 0 casas decimais', () => {
    expect(formatarMoeda(186.4, 5.42, 'R$', { comCentavos: false })).toBe('R$ 1.010')
  })
  it('R$ separa milhar com ponto a partir de 4 dígitos', () => {
    expect(formatarMoeda(1000, 1, 'R$')).toBe('R$ 1.000,00')
    expect(formatarMoeda(1234567.89, 1, 'R$')).toBe('R$ 1.234.567,89')
  })
  it('US$ separa milhar com vírgula a partir de 4 dígitos', () => {
    expect(formatarMoeda(12345.6, 1, 'US$')).toBe('US$ 12,345.60')
  })
  it('números abaixo de 1000 não ganham separador', () => {
    expect(formatarMoeda(186.4, 1, 'R$')).toBe('R$ 186,40')
    expect(formatarMoeda(186.4, 1, 'US$')).toBe('US$ 186.40')
  })
})

describe('formatarHora', () => {
  it('formata o início de hora como HHh, em UTC', () => {
    // 1_720_015_200 -> 03/jul/2024 14:00:00 UTC.
    expect(formatarHora(1_720_015_200)).toBe('14h')
  })
  it('zero-padding para horas de um dígito', () => {
    // 1_719_975_600 -> 03/jul/2024 03:00:00 UTC.
    expect(formatarHora(1_719_975_600)).toBe('03h')
  })
})

describe('formatarNumeroCompacto', () => {
  it('abrevia para k, M, B', () => {
    expect(formatarNumeroCompacto(999)).toBe('999')
    expect(formatarNumeroCompacto(1500)).toBe('1.5k')
    expect(formatarNumeroCompacto(2_500_000)).toBe('2.5M')
    expect(formatarNumeroCompacto(1_200_000_000)).toBe('1.2B')
  })
})

describe('formatarHorasMinutos', () => {
  it('formata horas e minutos com zero-padding', () => {
    expect(formatarHorasMinutos(0)).toBe('0h00m')
    expect(formatarHorasMinutos(65)).toBe('1h05m')
    expect(formatarHorasMinutos(90)).toBe('1h30m')
    expect(formatarHorasMinutos(3700)).toBe('61h40m')
  })
})

describe('corNivel', () => {
  it('vermelho para ERROR/CRIT/FATAL (todas as variantes)', () => {
    for (const n of ['ERROR', 'ERRO', 'CRIT', 'CRITICAL', 'FATAL']) {
      expect(corNivel(n)).toBe('var(--sev-vermelho)')
    }
  })
  it('accent-700 para WARNING', () => {
    expect(corNivel('WARNING')).toBe('var(--color-accent-700)')
    expect(corNivel('WARN')).toBe('var(--color-accent-700)')
  })
  it('neutro para INFO/DEBUG', () => {
    expect(corNivel('INFO')).toBe('var(--color-neutral-500)')
    expect(corNivel('DEBUG')).toBe('var(--color-neutral-500)')
  })
  it('case-insensitive', () => {
    expect(corNivel('error')).toBe('var(--sev-vermelho)')
    expect(corNivel('warning')).toBe('var(--color-accent-700)')
  })
})

describe('intensidadeParaCor', () => {
  it('mapeia 0..=5 para o ramp de cores do DS', () => {
    expect(intensidadeParaCor(0)).toBe('var(--color-neutral-200)')
    expect(intensidadeParaCor(1)).toBe('var(--color-accent-100)')
    expect(intensidadeParaCor(2)).toBe('var(--color-accent-200)')
    expect(intensidadeParaCor(3)).toBe('var(--color-accent-400)')
    expect(intensidadeParaCor(4)).toBe('var(--color-accent-600)')
    expect(intensidadeParaCor(5)).toBe('#a2503c')
  })
  it('fora da faixa cai no neutro (defensivo)', () => {
    expect(intensidadeParaCor(99)).toBe('var(--color-neutral-200)')
    expect(intensidadeParaCor(-1)).toBe('var(--color-neutral-200)')
  })
})
