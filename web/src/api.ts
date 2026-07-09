// Cliente da API do dev-server. Só fetch nativo — sem axios/react-query:
// para este portal, o fetch + polling da App resolve (YAGNI).
// As URLs são RELATIVAS (/api/...): em dev o proxy do Vite repassa ao
// dev-server; em produção portal e API saem da mesma origem (ServeDir).

import type { Alerta, ContainerResumo, LinhaLog } from './tipos'

/// GET + parse de JSON com erro para status não-2xx.
/// Genérica em T: o chamador diz o tipo esperado do corpo.
/// docs: https://developer.mozilla.org/docs/Web/API/Fetch_API
async function buscarJson<T>(caminho: string): Promise<T> {
  const resposta = await fetch(caminho)
  if (!resposta.ok) {
    throw new Error(`API respondeu ${resposta.status} em ${caminho}`)
  }
  return resposta.json() as Promise<T>
}

/// Containers ranqueados por severidade (a ORDENAÇÃO vem do servidor).
export function buscarContainers(janelaMin?: number): Promise<ContainerResumo[]> {
  const query = janelaMin !== undefined ? `?janela_min=${janelaMin}` : ''
  return buscarJson(`/api/containers${query}`)
}

/// Linhas de log de um container, opcionalmente filtradas por nível.
/// `encodeURIComponent`: nomes de container entram no PATH da URL.
export function buscarLinhas(
  nome: string,
  nivel?: string,
  limite: number = 100,
): Promise<LinhaLog[]> {
  const params = new URLSearchParams({ limite: String(limite) })
  if (nivel) params.set('nivel', nivel)
  return buscarJson(`/api/containers/${encodeURIComponent(nome)}/linhas?${params}`)
}

/// Alertas recentes (containers que pararam/reiniciaram).
export function buscarAlertas(limite: number = 100): Promise<Alerta[]> {
  return buscarJson(`/api/alertas?limite=${limite}`)
}
