class Gmux < Formula
  desc "Terminal Git workflow tool for multi-env branch sync and GitLab MR automation"
  homepage "https://github.com/life2you/gmux"
  url "https://github.com/life2you/gmux/archive/refs/tags/v0.1.1.tar.gz"
  sha256 "c309c7a8d864d4648bc6b53f9083fdcbe3f9f35f2f1a5d63a1673a3ec1d80dcf"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gmux --version")
  end
end
