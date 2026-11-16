class Asn < Formula
  desc "Your Asana Inbox in the terminal, with vim keys and a scriptable CLI"
  homepage "https://github.com/lplotni/asana-tui"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.1.0/asn-v0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "1c4e53d52347d42d3f7d7a5c8096f087da32842fea655edb1505bc79b5e1829e"
    end
    on_intel do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.1.0/asn-v0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "eeb0c138cadb9ebc5edaf4ee0aaca89b42a99b11354fee956538c522b887ac54"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.1.0/asn-v0.1.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "88ffbc674340727467a324ec9d9de785de0b2d9bb85f4f83e1793865f079b7c3"
    end
    on_intel do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.1.0/asn-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "8e1a7a6488ed716aa4d15a8ff80ca2984622ee62f6f771566059e382ae1d19da"
    end
  end

  def install
    bin.install "asn"
  end

  test do
    assert_match "asn", shell_output("#{bin}/asn --version")
  end
end
