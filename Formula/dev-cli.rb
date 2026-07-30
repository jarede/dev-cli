class DevCli < Formula
  desc "Canivete suíço de linha de comando para tarefas de desenvolvimento"
  homepage "https://github.com/jarede/dev-cli"
  license "MIT"
  version "0.2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/jarede/dev-cli/releases/download/v0.2.0/dev-cli-v0.2.0-aarch64-apple-darwin.tar.gz"
      sha256 "ba019169dbdc4f7c948b0a4e358e37327a925a212d4f042803395c15983c8dad"
    else
      url "https://github.com/jarede/dev-cli/releases/download/v0.2.0/dev-cli-v0.2.0-x86_64-apple-darwin.tar.gz"
      sha256 "2695e1c3671d3ff9763b44b1bdcfd4f27a5cf01da7f97e7348efdaad5651642c"
    end
  end

  on_linux do
    url "https://github.com/jarede/dev-cli/releases/download/v0.2.0/dev-cli-v0.2.0-x86_64-unknown-linux-gnu.tar.gz"
    sha256 "b60f792f933c390cc06bac3d7da53a0fbb9f2ec3ab51d6cee35f7de40ea3ff93"
  end

  def install
    bin.install "dev-cli"
    bin.install "dev-server"
  end

  test do
    assert_match "dev-cli #{version}", shell_output("#{bin}/dev-cli version")
  end
end
