import type { Alerta, ContainerResumo, LinhaLog } from './tipos'

async function buscarJson<T>(caminho: string): Promise<T> {
  const resposta = await fetch(caminho)
  if (!resposta.ok) {
    throw new Error(`API respondeu ${resposta.status} em ${caminho}`)
  }
  return resposta.json() as Promise<T>
}

export function buscarContainers(janelaMin?: number): Promise<ContainerResumo[]> {
  const query = janelaMin !== undefined ? `?janela_min=${janelaMin}` : ''
  return buscarJson(`/api/containers${query}`)
}

export function buscarLinhas(
  nome: string,
  nivel?: string,
  limite: number = 100,
): Promise<LinhaLog[]> {
  const params = new URLSearchParams({ limite: String(limite) })
  if (nivel) params.set('nivel', nivel)
  return buscarJson(`/api/containers/${encodeURIComponent(nome)}/linhas?${params}`)
}

export function buscarAlertas(limite: number = 100): Promise<Alerta[]> {
  return buscarJson(`/api/alertas?limite=${limite}`)
}
