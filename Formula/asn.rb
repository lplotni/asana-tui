class Asn < Formula
  desc "Your Asana Inbox in the terminal, with vim keys and a scriptable CLI"
  homepage "https://github.com/lplotni/asana-tui"
  version "0.2.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.2.0/asn-v0.2.0-aarch64-apple-darwin.tar.gz"
      sha256 "2f8a4231c13fab524bc76d09ffad5f9e4d7cee78a6cddac91da868cbc4ef4cd7"
    end
    on_intel do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.2.0/asn-v0.2.0-x86_64-apple-darwin.tar.gz"
      sha256 "2a9d78d9049d4b7f77d7b1dcbc7be4c715a218e0fd13ec560e0f38226933d62f"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.2.0/asn-v0.2.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "78de8c71dd9e90c6cdc17acb722b6314a083efea6802bde78bbda52696ff3ad2"
    end
    on_intel do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.2.0/asn-v0.2.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "68f470dd8db68da7216670681648463a264dbf39212b45fa969703c9da41e559"
    end
  end

  def install
    bin.install "asn"
  end

  test do
    assert_match "asn", shell_output("#{bin}/asn --version")
  end
end
