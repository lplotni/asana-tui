class Asn < Formula
  desc "Your Asana Inbox in the terminal, with vim keys and a scriptable CLI"
  homepage "https://github.com/lplotni/asana-tui"
  version "0.4.1"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.4.1/asn-v0.4.1-aarch64-apple-darwin.tar.gz"
      sha256 "244e6cf829d664392bfb0ba55451dfdee1c59a72514394f4e1c1748a2b2377c1"
    end
    on_intel do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.4.1/asn-v0.4.1-x86_64-apple-darwin.tar.gz"
      sha256 "4f8692153efc74302c6c04e7c5e12219e1390c314969b38865cf647288d38916"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.4.1/asn-v0.4.1-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "df4d834c095dc32d97f76be0ddd94f4d4952feae8d48d28f0b02c4d5a0f7b033"
    end
    on_intel do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.4.1/asn-v0.4.1-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "e4c53bb163478170ac8dc944b306462ee1a1bfa0693e47ec62a9e03a799162f4"
    end
  end

  def install
    bin.install "asn"
  end

  test do
    assert_match "asn", shell_output("#{bin}/asn --version")
  end
end
