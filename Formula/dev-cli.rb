class DevCli < Formula
  desc "Canivete suíço de linha de comando para tarefas de desenvolvimento"
  homepage "https://github.com/jarede/dev-cli"
  license "MIT"
  version "0.1.1"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/jarede/dev-cli/releases/download/v0.1.1/dev-cli-v0.1.1-aarch64-apple-darwin.tar.gz"
      sha256 "ee6814486399789ce302fc2a53f5613540bed877d1b530dc2573077f80888140"
    else
      url "https://github.com/jarede/dev-cli/releases/download/v0.1.1/dev-cli-v0.1.1-x86_64-apple-darwin.tar.gz"
      sha256 "33acc41a633a185c527616fce8d8d6014f703fd9a95d57c1463dd03e478b7257"
    end
  end

  on_linux do
    url "https://github.com/jarede/dev-cli/releases/download/v0.1.1/dev-cli-v0.1.1-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "c02eb2070c010f874854a1b44a21fae643c0c3c801e48ecbc34a4a6cf1c9c999"
  end

  def install
    bin.install "dev-cli"
  end

  test do
    assert_match "dev-cli #{version}", shell_output("#{bin}/dev-cli version")
  end
end
