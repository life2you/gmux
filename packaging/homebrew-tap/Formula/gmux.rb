class Gmux < Formula
  desc "Terminal Git workflow tool for multi-env branch sync and GitLab MR automation"
  homepage "https://github.com/life2you/gmux"
  url "https://github.com/life2you/gmux/archive/refs/tags/v0.1.5.tar.gz"
  sha256 "30e11b106d1457c4db15dc11c2326529af2fc87402d938ec330f29290fb4715d"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gmux --version")
  end
end
