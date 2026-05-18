class Gmux < Formula
  desc "Terminal Git workflow tool for multi-env branch sync and GitLab MR automation"
  homepage "https://github.com/life2you/gmux"
  version "0.1.9"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/life2you/gmux/releases/download/v0.1.9/gmux-aarch64-apple-darwin.tar.gz"
      sha256 "1c451de3a5179b67c4ef648dbb46ae6a613c99f321f9b49820e8e83bcb63fc87"
    end

    on_intel do
      url "https://github.com/life2you/gmux/releases/download/v0.1.9/gmux-x86_64-apple-darwin.tar.gz"
      sha256 "e27f1bcb917a164fbe7291d298749f07b9b8db81edd03135c50eef55e67ae116"
    end
  end

  def install
    bin.install "gmux"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gmux --version")
  end
end
