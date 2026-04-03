class Lazypost < Formula
  desc "Spring MVC/Boot API scanner and debugger TUI"
  homepage "https://github.com/life2you/lazypost4J"
  url "https://github.com/life2you/lazypost4J/archive/refs/tags/v0.1.0.tar.gz"
  sha256 "5ca9177d89ca7a922a79d649034e99020eb1563e63ec946da6bd06a9f8671137"
  license any_of: ["MIT", "Apache-2.0"]

  depends_on "rust" => :build

  def install
    system "cargo", "install", *std_cargo_args(path: ".")
  end

  test do
    (testpath/"DemoController.java").write <<~JAVA
      import org.springframework.web.bind.annotation.GetMapping;
      import org.springframework.web.bind.annotation.PathVariable;
      import org.springframework.web.bind.annotation.RequestParam;
      import org.springframework.web.bind.annotation.RestController;

      @RestController
      class DemoController {
        @GetMapping("/brew-test/{id}")
        public String hello(@PathVariable String id, @RequestParam(defaultValue = "all") String q) {
          return id + q;
        }
      }
    JAVA

    output = shell_output("#{bin}/lazypost scan #{testpath} --json")
    assert_match "\"path\": \"/brew-test/{id}\"", output
    assert_match "\"http_method\": \"GET\"", output
  end
end
