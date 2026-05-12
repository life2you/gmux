class Gmux < Formula
  desc "Terminal Git workflow tool for multi-env branch sync and GitLab MR automation"
  homepage "https://github.com/life2you/gmux"
  version "0.1.7"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/life2you/gmux/releases/download/v0.1.7/gmux-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_ARM64_SHA256"
    end

    on_intel do
      url "https://github.com/life2you/gmux/releases/download/v0.1.7/gmux-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_X64_SHA256"
    end
  end

  def install
    bin.install "gmux"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gmux --version")
  end
end
