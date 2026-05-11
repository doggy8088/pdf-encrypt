class PdfEncrypt < Formula
  desc "Encrypt and decrypt PDF documents"
  homepage "https://github.com/doggy8088/pdf-encrypt"
  version "0.1.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/doggy8088/pdf-encrypt/releases/download/v#{version}/pdf-encrypt-aarch64-apple-darwin.tar.xz"
      sha256 "REPLACE_WITH_AARCH64_APPLE_DARWIN_SHA256"
    else
      url "https://github.com/doggy8088/pdf-encrypt/releases/download/v#{version}/pdf-encrypt-x86_64-apple-darwin.tar.xz"
      sha256 "REPLACE_WITH_X86_64_APPLE_DARWIN_SHA256"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/doggy8088/pdf-encrypt/releases/download/v#{version}/pdf-encrypt-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "REPLACE_WITH_AARCH64_LINUX_GNU_SHA256"
    else
      url "https://github.com/doggy8088/pdf-encrypt/releases/download/v#{version}/pdf-encrypt-x86_64-unknown-linux-gnu.tar.xz"
      sha256 "REPLACE_WITH_X86_64_LINUX_GNU_SHA256"
    end
  end

  def install
    bin.install "pdf-encrypt"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/pdf-encrypt --version")
  end
end
