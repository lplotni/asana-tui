class Asn < Formula
  desc "Your Asana Inbox in the terminal, with vim keys and a scriptable CLI"
  homepage "https://github.com/lplotni/asana-tui"
  version "0.3.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.3.0/asn-v0.3.0-aarch64-apple-darwin.tar.gz"
      sha256 "6b0944169760b5862846674c040fa98f832cee5fcb4fb3d58ce586ee51845662"
    end
    on_intel do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.3.0/asn-v0.3.0-x86_64-apple-darwin.tar.gz"
      sha256 "8afc5c23b3b00403f8085dd0e36eb85e861af1fcd3bd52ad655c0bff92c9223e"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.3.0/asn-v0.3.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "add20672438e24501069cdfd940d5d1890c695bcfc81bb10ab5b91c9f43770e2"
    end
    on_intel do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.3.0/asn-v0.3.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "f4a8f139a58966f9e5c2470e984fd80f839fea48bd1b80a85ca024f51bf460eb"
    end
  end

  def install
    bin.install "asn"
  end

  test do
    assert_match "asn", shell_output("#{bin}/asn --version")
  end
end
