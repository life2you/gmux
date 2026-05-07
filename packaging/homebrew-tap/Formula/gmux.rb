class Gmux < Formula
  desc "Terminal Git workflow tool for multi-env branch sync and GitLab MR automation"
  homepage "https://github.com/life2you/gmux"
  url "https://github.com/life2you/gmux/archive/refs/tags/v0.1.4.tar.gz"
  sha256 "9a3fe6b596ecc315bcc27bd6a2f9cbbeb6f00f72269584976e0190c8cf2338be"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/gmux --version")
  end
end
