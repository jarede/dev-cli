import { describe, expect, it } from 'vitest'
import { formatarHaQuanto, formatarNumero, formatarSegundos } from './formato'

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
  const agora = 1_000_000_000 * 1000

  it('segundos, minutos e horas', () => {
    expect(formatarHaQuanto(1_000_000_000 - 12, agora)).toBe('há 12s')
    expect(formatarHaQuanto(1_000_000_000 - 3 * 60, agora)).toBe('há 3min')
    expect(formatarHaQuanto(1_000_000_000 - 2 * 3600, agora)).toBe('há 2h')
  })
  it('zero ou negativo vira "nunca" (container nunca coletado)', () => {
    expect(formatarHaQuanto(0, agora)).toBe('nunca')
  })
})
