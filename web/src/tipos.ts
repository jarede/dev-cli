export type Severidade = 'Verde' | 'Amarelo' | 'Vermelho' | 'Parado'

export interface ContainerResumo {
  nome: string
  status: string
  uptime: string
  erros: number
  crits: number
  c5xx: number
  c4xx: number
  reqs: number
  p95_seg: number | null
  max_seg: number | null
  total_linhas: number
  ultima_coleta: number
  severidade: Severidade
}

export interface LinhaLog {
  nivel: string
  linha: string
  collected_at: number
}

export interface Alerta {
  container: string
  tipo: string
  mensagem: string
  criado_em: number
}
