// Preview: tela Configuração (somente-leitura, espelha GET /api/config).
// Stub devolve a config efetiva típica de produção (RHEL + systemd).
import { Configuracao } from 'web'

const CONFIG = {
  coleta: {
    intervalo_seg: 30,
    janela_min: 60,
    retencao_horas: 336,
    tail_inicial: 200,
    db: '/var/lib/dev-cli/dev.db',
    ssh: '',
  },
  limiares: {
    p95_lento_seg: 1.5,
    taxa_erro_pct: 5,
  },
  servidor: {
    bind: '127.0.0.1:8787',
    portal_dir: '/var/lib/dev-cli/portal',
  },
}

const fetchOriginal = window.fetch.bind(window)
window.fetch = (async (recurso: RequestInfo | URL, init?: RequestInit) => {
  if (String(recurso).includes('/api/config')) {
    return new Response(JSON.stringify(CONFIG), {
      status: 200,
      headers: { 'content-type': 'application/json' },
    })
  }
  return fetchOriginal(recurso, init)
}) as typeof fetch

export const DockerLocal = () => <Configuracao />
