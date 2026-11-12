# Homebrew formula for CodeAtlas. Published from the tap repository
# Gabions256/homebrew-tap as Formula/codeatlas.rb; see packaging/README.md.
# Installs the prebuilt release binary. The version and the four sha256
# values are filled in per release from the .sha256 files on GitHub.
class Codeatlas < Formula
  desc "Local code intelligence: symbols, calls and imports with their evidence"
  homepage "https://github.com/Gabions256/codeatlas"
  version "0.1.0"
  license "Apache-2.0"

  on_macos do
    on_arm do
      url "https://github.com/Gabions256/codeatlas/releases/download/v0.1.0/atlas-v0.1.0-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256_aarch64-apple-darwin"
    end
    on_intel do
      url "https://github.com/Gabions256/codeatlas/releases/download/v0.1.0/atlas-v0.1.0-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256_x86_64-apple-darwin"
    end
  end

  on_linux do
    on_intel do
      url "https://github.com/Gabions256/codeatlas/releases/download/v0.1.0/atlas-v0.1.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_SHA256_x86_64-unknown-linux-gnu"
    end
  end

  def install
    bin.install "atlas"
  end

  test do
    (testpath/"a.py").write "def a():\n    pass\n"
    system bin/"atlas", "scan", testpath
    assert_match "a", shell_output("#{bin}/atlas --project #{testpath} symbols list")
  end
end
