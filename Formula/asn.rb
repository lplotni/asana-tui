class Asn < Formula
  desc "Your Asana Inbox in the terminal, with vim keys and a scriptable CLI"
  homepage "https://github.com/lplotni/asana-tui"
  version "0.1.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.1.1/asn-v0.1.1-aarch64-apple-darwin.tar.gz"
      sha256 "807b6f5019199ad56c9f3c00498cfc8b1eeaf41d4d31c124d13d2d036523d07e"
    end
    on_intel do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.1.1/asn-v0.1.1-x86_64-apple-darwin.tar.gz"
      sha256 "b0cc84cfbb234b0c8d118e5fdcda7b75668ec5c12ceb90067b4408049a78f64c"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.1.1/asn-v0.1.1-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "5588836a8529a8a976ad4f0dd6384b668a76d1cc2d9b32d475c263ca53bdf5a1"
    end
    on_intel do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.1.1/asn-v0.1.1-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "09a84fa94c09e8d414593e32c68083784efdc9628c6ed0b021ca23484390da31"
    end
  end

  def install
    bin.install "asn"
  end

  test do
    assert_match "asn", shell_output("#{bin}/asn --version")
  end
end
