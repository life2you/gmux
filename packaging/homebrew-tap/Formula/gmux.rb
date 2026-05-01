class Gmux < Formula
  desc "Terminal Git workflow tool for multi-env branch sync and GitLab MR automation"
  homepage "https://github.com/life2you/gmux"
  url "https://github.com/life2you/gmux/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "e38f867290f57d83718ce077a28a3a2ce28f17ab6a6fb2658701aee3c2ec3a40"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", "--locked", *std_cargo_args(path: ".")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gmux --version")
  end
end
