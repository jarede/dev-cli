class DevCli < Formula
  desc "Canivete suíço de linha de comando para tarefas de desenvolvimento"
  homepage "https://github.com/jarede/dev-cli"
  license "MIT"
  version "0.1.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/jarede/dev-cli/releases/download/v0.1.0/dev-cli-v0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "fe0c073d428e49cf407b7aa85e19c9a0cc83686b389bc9ed11563918941602af"
    else
      url "https://github.com/jarede/dev-cli/releases/download/v0.1.0/dev-cli-v0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "9e6e9759c78acf2c68e5eea87e5c9cc1fa545991e215012ca876c0596266d82f"
    end
  end

  on_linux do
    url "https://github.com/jarede/dev-cli/releases/download/v0.1.0/dev-cli-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "2f41dd95c3419e5d2be65dd58f07bfef20880feb79cbf85d3a0b80e541db1e57"
  end

  def install
    bin.install "dev-cli"
  end

  test do
    assert_match "dev-cli #{version}", shell_output("#{bin}/dev-cli version")
  end
end
