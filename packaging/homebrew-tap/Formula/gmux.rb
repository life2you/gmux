class Gmux < Formula
  desc "Terminal Git workflow tool for multi-env branch sync and GitLab MR automation"
  homepage "https://github.com/life2you/gmux"
  version "0.1.8"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/life2you/gmux/releases/download/v0.1.8/gmux-aarch64-apple-darwin.tar.gz"
      sha256 "030573c7e16c80c3ceed16e249003993e7a9ad6cfa8d01cf1ae4ca8eb7e287d1"
    end

    on_intel do
      url "https://github.com/life2you/gmux/releases/download/v0.1.8/gmux-x86_64-apple-darwin.tar.gz"
      sha256 "fac47b520cad2e217035826551ef4274c9da52002c14f71416a98d313b46b4fb"
    end
  end

  def install
    bin.install "gmux"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gmux --version")
  end
end
