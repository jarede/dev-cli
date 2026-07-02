# Fórmula do Homebrew para o `dev-cli`, mantida neste próprio repositório
# (não precisa de um repo separado `homebrew-*`: `brew tap` aceita uma URL
# explícita — ver instruções de instalação no README).
#
# Este arquivo é reescrito automaticamente pelo workflow
# `.github/workflows/release.yml` a cada tag `vX.Y.Z` publicada — os
# valores abaixo (versão e sha256) são só um placeholder inicial e devem
# ser tratados como desatualizados até a primeira release rodar.
class DevCli < Formula
  desc "Canivete suíço de linha de comando para tarefas de desenvolvimento"
  homepage "https://github.com/jarede/dev-cli"
  license "MIT"
  version "0.1.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/jarede/dev-cli/releases/download/v0.1.0/dev-cli-v0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    else
      url "https://github.com/jarede/dev-cli/releases/download/v0.1.0/dev-cli-v0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "0000000000000000000000000000000000000000000000000000000000000000"
    end
  end

  on_linux do
    url "https://github.com/jarede/dev-cli/releases/download/v0.1.0/dev-cli-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "0000000000000000000000000000000000000000000000000000000000000000"
  end

  def install
    bin.install "dev-cli"
  end

  test do
    assert_match "dev-cli #{version}", shell_output("#{bin}/dev-cli version")
  end
end
