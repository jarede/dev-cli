// Funções PURAS de formatação: valor -> string, sem DOM e sem estado —
// o equivalente web do "núcleo puro" do crates/nucleo, 100% testável.

/// Segundos com 2 casas ("1.23s"); null (sem dados) vira travessão.
export function formatarSegundos(valor: number | null): string {
  if (valor === null) return '—'
  return `${valor.toFixed(2)}s`
}

/// Abrevia milhares: 1234 -> "1.2k". Mantém < 1000 como está.
export function formatarNumero(valor: number): string {
  if (valor >= 1000) return `${(valor / 1000).toFixed(1)}k`
  return String(valor)
}

/// Quanto tempo faz desde um timestamp Unix (segundos): "há 12s"/"há 3min"/
/// "há 2h". `agoraMs` é injetável para os testes serem determinísticos
/// (em produção usa Date.now()).
export function formatarHaQuanto(tsUnix: number, agoraMs: number = Date.now()): string {
  if (tsUnix <= 0) return 'nunca'
  const seg = Math.max(0, Math.floor(agoraMs / 1000) - tsUnix)
  if (seg < 60) return `há ${seg}s`
  if (seg < 3600) return `há ${Math.floor(seg / 60)}min`
  return `há ${Math.floor(seg / 3600)}h`
}

/**
 * Formata um timestamp Unix (segundos, sempre início de hora — múltiplo
 * de 3600, como `CelulaHistorico.hora`) como "HHh" — usado no tooltip do
 * strip de Histórico ("14h · 5 erros/críticos"). Usa UTC (`getUTCHours`),
 * não o fuso local: o agrupamento por hora no servidor
 * (`(collected_at / 3600) * 3600`) já opera sobre o timestamp Unix cru, ou
 * seja, em fronteiras de hora UTC — formatar em UTC aqui mantém o rótulo
 * consistente com a fronteira real da célula (e a função determinística
 * nos testes, sem depender do fuso horário de quem roda `npm test`).
 */
export function formatarHora(tsUnix: number): string {
  const d = new Date(tsUnix * 1000)
  return `${String(d.getUTCHours()).padStart(2, '0')}h`
}

/** Nomes "longos" de moeda para a UI da tela IA · custos. */
export type Moeda = 'US$' | 'R$'

/**
 * Insere separador de milhar numa string numérica não-negativa já formatada
 * com ponto decimal (ex.: "1234.56" -> comSeparador "." dá "1.234,56" com
 * `separadorDecimal: ','`). Regex pura, sem `Intl.NumberFormat`, de
 * propósito — o objetivo é manter `formatarMoeda` 100% determinística nos
 * testes, sem depender do locale/ICU disponível no ambiente que roda
 * `npm test` (CI, máquinas diferentes...).
 * `\B(?=(\d{3})+(?!\d))`: casa toda posição ENTRE dígitos (`\B`) que tenha
 * um múltiplo de 3 dígitos até o fim da parte inteira à frente — é onde o
 * separador de milhar entra.
 */
function comSeparadorDeMilhar(
  valorAbsoluto: string,
  separadorMilhar: string,
  separadorDecimal: string,
): string {
  const [inteiro, decimal] = valorAbsoluto.split('.')
  const inteiroComSeparador = inteiro.replace(/\B(?=(\d{3})+(?!\d))/g, separadorMilhar)
  return decimal !== undefined ? `${inteiroComSeparador}${separadorDecimal}${decimal}` : inteiroComSeparador
}

/**
 * Formata um valor em USD na moeda pedida, com separador de milhar (pt-br:
 * "1.010,29"; US$: "1,010.40"). Padrão "R$" (escolha do usuário ao revisar
 * o protótipo) — `moeda: 'US$'` mostra o valor original e o equivalente em
 * R$ na linha secundária.
 */
export function formatarMoeda(
  usd: number,
  cambio: number,
  moeda: Moeda = 'R$',
  opcoes: { comCentavos?: boolean } = {},
): string {
  const { comCentavos = true } = opcoes
  const valor = moeda === 'US$' ? usd : usd * cambio
  const sinal = valor < 0 ? '-' : ''
  const bruto = Math.abs(valor).toFixed(comCentavos ? 2 : 0)
  if (moeda === 'US$') {
    return `US$ ${sinal}${comSeparadorDeMilhar(bruto, ',', '.')}`
  }
  return `R$ ${sinal}${comSeparadorDeMilhar(bruto, '.', ',')}`
}

/// Navegação de meses no formato "YYYY-MM" — aritmética direta em números
/// para não depender de Date (fuso/dia do mês não importam aqui).
export function mesAnterior(mes: string): string {
  const [ano, m] = mes.split('-').map(Number)
  return m === 1 ? `${ano - 1}-12` : `${ano}-${String(m - 1).padStart(2, '0')}`
}

export function mesSeguinte(mes: string): string {
  const [ano, m] = mes.split('-').map(Number)
  return m === 12 ? `${ano + 1}-01` : `${ano}-${String(m + 1).padStart(2, '0')}`
}

/// "2026-07" → "julho de 2026". Intl faz a tradução do nome do mês; o
/// dia 2 evita qualquer surpresa de fuso (dia 1 UTC pode cair no mês
/// anterior em fusos negativos como o do Brasil).
/// docs: https://developer.mozilla.org/docs/Web/JavaScript/Reference/Global_Objects/Intl/DateTimeFormat
export function mesPorExtenso(mes: string): string {
  const data = new Date(`${mes}-02T12:00:00`)
  return new Intl.DateTimeFormat('pt-BR', { month: 'long', year: 'numeric' }).format(data)
}

/// Mês atual local em "YYYY-MM" — o estado inicial do seletor da tela IA.
export function mesAtual(): string {
  const agora = new Date()
  return `${agora.getFullYear()}-${String(agora.getMonth() + 1).padStart(2, '0')}`
}

/** Compacta um número grande com sufixo: 1_234_567 -> "1.2M". */
export function formatarNumeroCompacto(valor: number): string {
  if (valor >= 1_000_000_000) return `${(valor / 1_000_000_000).toFixed(1)}B`
  if (valor >= 1_000_000) return `${(valor / 1_000_000).toFixed(1)}M`
  if (valor >= 1_000) return `${(valor / 1_000).toFixed(1)}k`
  return String(valor)
}

/** Minutos → "HhMm": 65 → "1h05m", 90 → "1h30m". */
export function formatarHorasMinutos(minutos: number): string {
  if (minutos <= 0) return '0h00m'
  const h = Math.floor(minutos / 60)
  const m = Math.round(minutos % 60)
  return `${h}h${String(m).padStart(2, '0')}m`
}

/**
 * Cor CSS de uma severidade (variáveis do index.css).
 * Record: força TODAS as variantes de `Severidade` no map — o TS acusa
 * se o portal acrescentar uma severidade nova e esquecer a cor.
 * docs: https://www.typescriptlang.org/docs/handbook/utility-types.html#recordkeys-type
 */
export const COR_SEVERIDADE: Record<import('./tipos').Severidade, string> = {
  Verde: 'var(--sev-verde)',
  Amarelo: 'var(--sev-amarelo)',
  Vermelho: 'var(--sev-vermelho)',
  Parado: 'var(--sev-parado)',
}

/**
 * Cor CSS para um nível de log (string crua do banco: ERROR, INFO...).
 * Centraliza o mapeamento que estava espalhado entre FeedErros e
 * PainelContainer — agora uma alteração aqui reflete nas duas telas.
 *
 * A lista de níveis "erro/crítico" abaixo é uma cópia PROPOSITAL da
 * constante `nucleo::db::NIVEIS_ERRO` (Rust) — cruzar a fronteira
 * TS/Rust com uma constante compartilhada de verdade exigiria gerar este
 * arquivo ou embutir o JSON em build, o que não vale a pena para 5
 * strings. `nucleo::db::NIVEIS_ERRO` é a FONTE DA VERDADE; se um nível
 * novo for adicionado lá (`erros_desde`/`historico_por_hora`), atualize
 * aqui também.
 */
export function corNivel(nivel: string): string {
  const n = nivel.toUpperCase()
  if (n === 'ERROR' || n === 'ERRO' || n === 'CRIT' || n === 'CRITICAL' || n === 'FATAL') {
    return 'var(--sev-vermelho)'
  }
  if (n === 'WARNING' || n === 'WARN') return 'var(--color-accent-700)'
  return 'var(--color-neutral-500)'
}

/**
 * Mapeia uma intensidade 0..=5 (escala do heatmap) para a cor CSS
 * correspondente. Mesma escala do protótipo .dc.html: vai do neutro
 * 200 (sem atividade) até vermelho-tijolo (pico).
 */
export function intensidadeParaCor(intensidade: number): string {
  switch (intensidade) {
    case 0: return 'var(--color-neutral-200)'
    case 1: return 'var(--color-accent-100)'
    case 2: return 'var(--color-accent-200)'
    case 3: return 'var(--color-accent-400)'
    case 4: return 'var(--color-accent-600)'
    case 5: return 'var(--sev-vermelho)'
    default: return 'var(--color-neutral-200)'
  }
}
