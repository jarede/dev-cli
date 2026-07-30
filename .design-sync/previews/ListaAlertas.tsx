// Preview: card de Alertas — some quando vazio (por isso não há story "vazio";
// o comportamento vazio = null é documentado no .prompt.md).
import { ListaAlertas } from 'web'

const agora = Math.floor(Date.now() / 1000)

export const DoisAlertas = () => (
  <ListaAlertas
    alertas={[
      {
        container: 'supply',
        tipo: 'parado',
        mensagem: 'supply parou (exit code 137) — sem coleta desde então',
        criado_em: agora - 3 * 3600,
      },
      {
        container: 'prezzo',
        tipo: 'reinicio',
        mensagem: 'prezzo reiniciou 2 vezes na última hora',
        criado_em: agora - 40 * 60,
      },
    ]}
  />
)

export const UmAlerta = () => (
  <ListaAlertas
    alertas={[
      {
        container: 'ecomm',
        tipo: 'reinicio',
        mensagem: 'ecomm reiniciou após OOM (limite de memória do compose)',
        criado_em: agora - 8 * 60,
      },
    ]}
  />
)
