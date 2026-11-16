#!/bin/sh
# Prints the Homebrew formula for a release:
#   scripts/formula.sh VERSION SHA_MAC_ARM SHA_MAC_INTEL SHA_LINUX_ARM SHA_LINUX_INTEL
set -eu
[ $# -eq 5 ] || { echo "usage: $0 VERSION SHA_MAC_ARM SHA_MAC_INTEL SHA_LINUX_ARM SHA_LINUX_INTEL" >&2; exit 2; }
version=$1
base="https://github.com/lplotni/asana-tui/releases/download/v$version"
cat <<FORMULA
class Asn < Formula
  desc "Your Asana Inbox in the terminal, with vim keys and a scriptable CLI"
  homepage "https://github.com/lplotni/asana-tui"
  version "$version"

  on_macos do
    on_arm do
      url "$base/asn-v$version-aarch64-apple-darwin.tar.gz"
      sha256 "$2"
    end
    on_intel do
      url "$base/asn-v$version-x86_64-apple-darwin.tar.gz"
      sha256 "$3"
    end
  end

  on_linux do
    on_arm do
      url "$base/asn-v$version-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "$4"
    end
    on_intel do
      url "$base/asn-v$version-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "$5"
    end
  end

  def install
    bin.install "asn"
  end

  test do
    assert_match "asn", shell_output("#{bin}/asn --version")
  end
end
FORMULA
