package demo;

import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/api/v1/demo")
public class DemoController {

    /**
     * 根据路径 id 返回问候语；q 为可选查询关键词。
     */
    @GetMapping("/hello/{id}")
    public String hello(
            @PathVariable("id") String id,
            @RequestParam(name = "q", defaultValue = "all") String q) {
        return "hello";
    }

    public static class EchoBody {
        public String text;
    }

    /**
     * 接收 JSON 体并回显其中 text 字段。
     */
    @PostMapping("/echo")
    public String echo(@RequestBody EchoBody body) {
        return body.text;
    }
}
