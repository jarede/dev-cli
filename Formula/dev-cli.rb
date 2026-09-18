class DevCli < Formula
  desc "Canivete suíço de linha de comando para tarefas de desenvolvimento"
  homepage "https://github.com/jarede/dev-cli"
  license "MIT"
  version "0.3.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/jarede/dev-cli/releases/download/v0.3.0/dev-cli-v0.3.0-aarch64-apple-darwin.tar.gz"
      sha256 "71c87793c676863a7c75dfcbb8a7bb202b58b7c7fc179b93fc193a1621d21129"
    else
      url "https://github.com/jarede/dev-cli/releases/download/v0.3.0/dev-cli-v0.3.0-x86_64-apple-darwin.tar.gz"
      sha256 "22912338b12f103c338124aaf1dd90bc3aa0c31a9e44317ad50da0b49a5fcd8e"
    end
  end

  on_linux do
    url "https://github.com/jarede/dev-cli/releases/download/v0.3.0/dev-cli-v0.3.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "7772b90af799175ee4c5e32d87ab2c216ce92adc18242041d75a3b97250fdbe3"
  end

  def install
    bin.install "dev-cli"
    bin.install "dev-server"
  end

  test do
    assert_match "dev-cli #{version}", shell_output("#{bin}/dev-cli version")
  end
end
