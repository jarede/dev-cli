// Tipos espelhando o JSON da API do dev-server (Fase 2). Os nomes dos
// campos são EXATAMENTE as chaves do JSON (vêm dos structs Rust com
// Serialize) — não renomear.
// docs: https://www.typescriptlang.org/docs/handbook/2/objects.html

/// A severidade calculada pelo servidor (o portal não reclassifica).
export type Severidade = 'Verde' | 'Amarelo' | 'Vermelho' | 'Parado'

/// Um container em /api/containers — resumo da janela + severidade.
export interface ContainerResumo {
  nome: string
  status: string
  uptime: string
  erros: number
  crits: number
  c5xx: number
  c4xx: number
  reqs: number
  /** p95 do tempo de resposta em segundos; null sem requests na janela. */
  p95_seg: number | null
  max_seg: number | null
  total_linhas: number
  /** Timestamp Unix (segundos) da última coleta deste container. */
  ultima_coleta: number
  severidade: Severidade
}

/// Uma linha de log em /api/containers/{nome}/linhas.
export interface LinhaLog {
  nivel: string
  linha: string
  collected_at: number
}

/// Um alerta (container parou/reiniciou) em /api/alertas.
export interface Alerta {
  container: string
  tipo: string
  mensagem: string
  criado_em: number
}
