class Asn < Formula
  desc "Your Asana Inbox in the terminal, with vim keys and a scriptable CLI"
  homepage "https://github.com/lplotni/asana-tui"
  version "0.4.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.4.0/asn-v0.4.0-aarch64-apple-darwin.tar.gz"
      sha256 "18c3a4ee1d470c6c450a4dfb046b53c63cbe1b5ff6638b1d93be60e5c59ff69b"
    end
    on_intel do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.4.0/asn-v0.4.0-x86_64-apple-darwin.tar.gz"
      sha256 "b6b9acd055d3df2152bf7c6290c54fd42c9535465357b2dd2637f3832b0d968a"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.4.0/asn-v0.4.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "0c28a5c5f548d4a5dfeac302e3f585416ea82fd4e2a9620e8a8463f5cd1fb3a3"
    end
    on_intel do
      url "https://github.com/lplotni/asana-tui/releases/download/v0.4.0/asn-v0.4.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "368f93f0f76e4476302bb197bd969caea72879eed1ec771fba4372a9c68ac73a"
    end
  end

  def install
    bin.install "asn"
  end

  test do
    assert_match "asn", shell_output("#{bin}/asn --version")
  end
end
