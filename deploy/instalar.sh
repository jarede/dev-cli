#!/usr/bin/env bash
# Instala o dev-server e a coleta periódica do panorama como serviços
# systemd numa VM RHEL-like.
#
# Uso (na raiz do repositório, como root):
#   cargo build --release
#   sudo ./deploy/instalar.sh [BIN_SERVIDOR] [BIN_CLI]
#
# O que faz: cria o usuário de sistema `dev-cli` (sem shell, membro do grupo
# docker), instala os binários em /usr/local/bin, a config em /etc/dev-cli e
# as units em /etc/systemd/system, e habilita o serviço + o timer.
#
# Compatibilidade: o primeiro argumento posicional continua sendo o binário
# do `dev-server`; o segundo (novo) é o binário do `dev-cli` que o timer
# `panorama-coletar` chama. Quem já chamava com um argumento só não quebra.
set -euo pipefail

BIN_SERVIDOR="${1:-target/release/dev-server}"
BIN_CLI="${2:-target/release/dev-cli}"
USUARIO="dev-cli"

if [[ $EUID -ne 0 ]]; then
  echo "erro: rode como root (sudo ./deploy/instalar.sh)" >&2
  exit 1
fi
if [[ ! -f "$BIN_SERVIDOR" ]]; then
  echo "erro: binário não encontrado em $BIN_SERVIDOR — rode 'cargo build --release' antes" >&2
  exit 1
fi
if [[ ! -f "$BIN_CLI" ]]; then
  echo "erro: binário não encontrado em $BIN_CLI — rode 'cargo build --release' antes" >&2
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

# 2. Binários e configuração (a config só é copiada se ainda não existir,
# para um upgrade não sobrescrever ajustes do operador; o panorama.env
# NUNCA é criado aqui — o operador o cria à mão com o token do GitLab).
install -m 0755 "$BIN_SERVIDOR" /usr/local/bin/dev-server
install -m 0755 "$BIN_CLI" /usr/local/bin/dev-cli
mkdir -p /etc/dev-cli
if [[ ! -f /etc/dev-cli/config.toml ]]; then
  install -m 0644 deploy/config.exemplo.toml /etc/dev-cli/config.toml
  echo "config instalada em /etc/dev-cli/config.toml"
fi

# 3. SELinux (RHEL): garante o contexto padrão dos binários recém-copiados.
if command -v restorecon &>/dev/null; then
  restorecon /usr/local/bin/dev-server /usr/local/bin/dev-cli
fi

# 4. Units do systemd (o StateDirectory do dev-server cria /var/lib/dev-cli
# na primeira subida, já com dono dev-cli; o diretório de snapshots do
# panorama precisa existir ANTES de habilitar o timer, porque o
# ReadWritePaths da unit (sem `-`) exige o caminho — senão a coleta falha).
install -m 0644 deploy/dev-server.service /etc/systemd/system/dev-server.service
install -m 0644 deploy/panorama-coletar.service /etc/systemd/system/panorama-coletar.service
install -m 0644 deploy/panorama-coletar.timer /etc/systemd/system/panorama-coletar.timer
install -d -o "$USUARIO" -g "$USUARIO" /var/lib/panorama/snapshots
systemctl daemon-reload
systemctl enable --now dev-server
systemctl enable --now panorama-coletar.timer

echo
systemctl status dev-server --no-pager || true
echo
systemctl list-timers --no-pager panorama-coletar.timer || true
echo
echo "pronto. teste com: curl -s localhost:8787/api/saude"
