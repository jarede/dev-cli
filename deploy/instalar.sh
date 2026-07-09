#!/usr/bin/env bash
# Instala o dev-server como serviço systemd numa VM RHEL-like.
#
# Uso (na raiz do repositório, como root):
#   cargo build --release
#   sudo ./deploy/instalar.sh
#
# O que faz: cria o usuário de sistema `dev-cli` (sem shell, membro do grupo
# docker), instala o binário em /usr/local/bin, a config em /etc/dev-cli e a
# unit em /etc/systemd/system, e habilita o serviço.
set -euo pipefail

BIN_ORIGEM="${1:-target/release/dev-server}"
USUARIO="dev-cli"

if [[ $EUID -ne 0 ]]; then
  echo "erro: rode como root (sudo ./deploy/instalar.sh)" >&2
  exit 1
fi
if [[ ! -f "$BIN_ORIGEM" ]]; then
  echo "erro: binário não encontrado em $BIN_ORIGEM — rode 'cargo build --release' antes" >&2
  exit 1
fi
if ! getent group docker >/dev/null; then
  echo "erro: grupo 'docker' não existe — instale/inicie o docker antes" >&2
  exit 1
fi

# 1. Usuário de serviço: de sistema, sem home, sem shell de login.
if ! id "$USUARIO" &>/dev/null; then
  useradd --system --no-create-home --shell /usr/sbin/nologin "$USUARIO"
  echo "usuário '$USUARIO' criado"
fi
usermod -aG docker "$USUARIO"

# 2. Binário e configuração (a config só é copiada se ainda não existir,
# para um upgrade não sobrescrever ajustes do operador).
install -m 0755 "$BIN_ORIGEM" /usr/local/bin/dev-server
mkdir -p /etc/dev-cli
if [[ ! -f /etc/dev-cli/config.toml ]]; then
  install -m 0644 deploy/config.exemplo.toml /etc/dev-cli/config.toml
  echo "config instalada em /etc/dev-cli/config.toml"
fi

# 3. SELinux (RHEL): garante o contexto padrão do binário recém-copiado.
if command -v restorecon &>/dev/null; then
  restorecon /usr/local/bin/dev-server
fi

# 4. Unit do systemd (o StateDirectory cria /var/lib/dev-cli na primeira
# subida, já com dono dev-cli).
install -m 0644 deploy/dev-server.service /etc/systemd/system/dev-server.service
systemctl daemon-reload
systemctl enable --now dev-server

echo
systemctl status dev-server --no-pager || true
echo
echo "pronto. teste com: curl -s localhost:8787/api/saude"
